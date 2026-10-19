//! Owns the picker, sealed payload registry and provider command construction.

use super::*;
use tokio::task::JoinHandle;

struct FrozenSend {
    content: String,
    reply_to: Option<MessageId>,
    mention_reply: bool,
}

#[cfg(any(target_os = "linux", windows))]
type Chooser = rune_platform::file_dialogs::FileChooser;
#[cfg(not(any(target_os = "linux", windows)))]
struct Chooser;

struct PickDone {
    chooser: Option<Chooser>,
    worker: prepare::PrepareWorker,
    account: UserId,
    channel: ChannelId,
    request: RequestId,
    token: UploadToken,
    result: prepare::Prepared,
    /// Where each prepared file was read from, when known.
    origins: Vec<PathBuf>,
    /// The kept files a restore left out, by name.
    skipped: Vec<String>,
}

async fn pick_file(
    mut chooser: Option<Chooser>,
    mut worker: prepare::PrepareWorker,
    job: Job,
    budget: Arc<prepare::Budget>,
    limits: UploadLimits,
) -> PickDone {
    let Job::Pick {
        account,
        channel,
        request,
        token,
        capacity,
        max_total_bytes,
        input,
        cancel,
    } = job
    else {
        unreachable!()
    };
    let mut skipped = Vec::new();
    let (result, origins) = async {
        worker.wait_idle(cancel.clone()).await?;
        let source = match input {
            PickInput::Restore(files) => {
                // An earlier dialog's result first, as for paths.
                #[cfg(any(target_os = "linux", windows))]
                if let Some(chooser) = chooser.as_mut() {
                    chooser
                        .wait_idle(cancel.clone())
                        .await
                        .map_err(pick_error)?;
                }
                return Ok(restore(
                    &mut worker,
                    files,
                    token,
                    capacity,
                    &budget,
                    limits.max_file_bytes.min(prepare::MEMORY_BYTES as u64),
                    max_total_bytes,
                    cancel,
                    &mut skipped,
                )
                .await);
            }
            PickInput::Paths(paths) => {
                // A previous cancelled dialog can still own a filesystem
                // result. Drain that exact slot without connecting a portal.
                #[cfg(any(target_os = "linux", windows))]
                if let Some(chooser) = chooser.as_mut() {
                    chooser
                        .wait_idle(cancel.clone())
                        .await
                        .map_err(pick_error)?;
                }
                prepare::Source::Paths {
                    paths,
                    max_files: capacity,
                    max_file_bytes: limits.max_file_bytes.min(prepare::MEMORY_BYTES as u64),
                    max_total_bytes,
                }
            }
            PickInput::Clipboard => {
                // The same wait as Paths: an earlier dialog's result first.
                #[cfg(any(target_os = "linux", windows))]
                if let Some(chooser) = chooser.as_mut() {
                    chooser
                        .wait_idle(cancel.clone())
                        .await
                        .map_err(pick_error)?;
                }
                prepare::Source::Clipboard {
                    max_files: capacity,
                    max_file_bytes: limits.max_file_bytes.min(prepare::MEMORY_BYTES as u64),
                    max_total_bytes,
                }
            }
            PickInput::Picker(parent) => {
                #[cfg(any(target_os = "linux", windows))]
                {
                    use rune_platform::file_dialogs::{FileChooser, PickId, PickRequest};
                    if chooser.is_none() {
                        chooser = Some(FileChooser::session().await.map_err(pick_error)?);
                    }
                    let request = PickRequest {
                        id: PickId(request),
                        max_files: capacity,
                        max_file_bytes: limits.max_file_bytes.min(prepare::MEMORY_BYTES as u64),
                        max_total_bytes,
                        parent,
                        current_folder: None,
                    };
                    prepare::Source::Selected(
                        chooser
                            .as_mut()
                            .unwrap()
                            .choose_files(&request, cancel.clone())
                            .await
                            .map_err(pick_error)?,
                    )
                }
                #[cfg(not(any(target_os = "linux", windows)))]
                {
                    let _ = parent;
                    return Err(UploadError::Unavailable);
                }
            }
        };
        Ok(worker.prepare(source, token, &budget, cancel).await)
    }
    .await
    .unwrap_or_else(|error| (Err(error), Vec::new()));
    PickDone {
        chooser,
        worker,
        account,
        channel,
        request,
        token,
        result,
        origins,
        skipped,
    }
}

/// The files a draft kept, each opened again alone with every check: one
/// gone, unreadable or of another size than kept is left out and named in
/// `skipped` (an empty name for one without a name), as are the ones past
/// `capacity`; the rest come back with consecutive tokens from `token`.
/// Short of memory or of the worker, none comes back and none is named as
/// gone (only that error is), so the draft keeps them all for the next
/// run: Core checked the room before, so only a race gets here.
#[allow(clippy::too_many_arguments)]
async fn restore(
    worker: &mut prepare::PrepareWorker,
    files: Vec<rune_storage::DraftFile>,
    token: UploadToken,
    capacity: usize,
    budget: &Arc<prepare::Budget>,
    max_file_bytes: u64,
    max_total_bytes: u64,
    cancel: watch::Receiver<bool>,
    skipped: &mut Vec<String>,
) -> prepare::Answer {
    let mut prepared = Vec::new();
    let mut origins = Vec::new();
    let mut room = max_total_bytes;
    for kept in files {
        let path = PathBuf::from(&kept.path);
        if prepared.len() == capacity {
            skipped.push(shown_name(&path));
            continue;
        }
        let source = prepare::Source::Paths {
            paths: vec![path.clone()],
            max_files: 1,
            max_file_bytes,
            max_total_bytes: room,
        };
        let next = UploadToken(token.0 + prepared.len() as u64);
        match worker.prepare(source, next, budget, cancel.clone()).await {
            (Ok(mut files), _)
                if files.len() == 1 && files[0].metadata().size_bytes == kept.size_bytes =>
            {
                room = room.saturating_sub(kept.size_bytes);
                prepared.append(&mut files);
                origins.push(path);
            }
            _ if prepare::cancelled(&cancel) => return (Err(UploadError::Cancelled), Vec::new()),
            (Ok(_), _)
            | (
                Err(
                    UploadError::NotFound
                    | UploadError::InvalidFile
                    | UploadError::Denied
                    | UploadError::TooLarge
                    | UploadError::Changed
                    | UploadError::Failed,
                ),
                _,
            ) => skipped.push(shown_name(&path)),
            (Err(error), _) => {
                skipped.clear();
                return (Err(error), Vec::new());
            }
        }
    }
    if prepared.is_empty() {
        // Nothing came back: what was left out is all there is to say.
        return (Err(UploadError::Cancelled), Vec::new());
    }
    (Ok(prepared), origins)
}

/// A kept file's name, as a person would recognize it; empty for one
/// without a name, which the interface names in its language.
fn shown_name(path: &std::path::Path) -> String {
    let name: String = path
        .file_name()
        .map(|name| {
            name.to_string_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .take(80)
                .collect()
        })
        .unwrap_or_default();
    if name.trim().is_empty() {
        String::new()
    } else {
        name
    }
}

/// The save or folder dialog on the shared chooser; the chooser comes back
/// with it.
async fn choose_destination(mut chooser: Option<Chooser>, job: Job) -> Option<Chooser> {
    let Job::Save {
        request,
        cancel,
        reply,
    } = job
    else {
        unreachable!()
    };
    #[cfg(any(target_os = "linux", windows))]
    let result = async {
        use rune_platform::file_dialogs::FileChooser;
        if chooser.is_none() {
            chooser = Some(FileChooser::session().await.map_err(pick_error)?);
        }
        let chooser = chooser.as_mut().unwrap();
        // A cancelled earlier dialog may still own its slot: drain it first.
        chooser
            .wait_idle(cancel.clone())
            .await
            .map_err(pick_error)?;
        match &request {
            Destination::File(request) => chooser
                .save_file(request, cancel)
                .await
                .map(rune_platform::file_dialogs::SaveDestination::into_path),
            Destination::Folder(request) => chooser.choose_folder(request, cancel).await,
        }
        .map_err(pick_error)
    }
    .await;
    #[cfg(not(any(target_os = "linux", windows)))]
    let result = {
        let _ = (request, cancel);
        Err(UploadError::Unavailable)
    };
    let _ = reply.send(result);
    chooser
}

pub(super) async fn run(
    shared: Arc<Mutex<Control>>,
    changed: Arc<Notify>,
    mut jobs: mpsc::Receiver<Job>,
    commands: mpsc::Sender<Command>,
    limits: Option<UploadLimits>,
    wake: Arc<dyn Fn() + Send + Sync>,
    budget: Arc<prepare::Budget>,
) {
    let mut worker = limits.and_then(|_| prepare::PrepareWorker::new().ok());
    let mut chooser = None;
    let mut picking: Option<JoinHandle<PickDone>> = None;
    let mut saving: Option<JoinHandle<Option<Chooser>>> = None;
    let mut entries = HashMap::<UploadToken, PreparedUpload>::new();
    let mut sends = HashMap::<u64, FrozenSend>::new();
    loop {
        let (tokens, nonces, closed) = {
            let state = shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (
                state.tokens.keys().copied().collect::<Vec<_>>(),
                state.sends.keys().copied().collect::<Vec<_>>(),
                state.closed,
            )
        };
        // Releasing a last buffer owner can deallocate MiBs. Never hold the
        // small UI control lock while dropping payloads or frozen bodies.
        entries.retain(|token, _| tokens.contains(token));
        sends.retain(|nonce, _| nonces.contains(nonce));
        if closed {
            break;
        }
        tokio::select! {
            biased;
            _ = changed.notified() => {},
            _ = async { worker.as_mut().unwrap().discard_pending().await },
                if picking.is_none() && worker.as_ref().is_some_and(prepare::PrepareWorker::has_pending) => {},
            done = async { saving.as_mut().unwrap().await }, if saving.is_some() => {
                saving = None;
                chooser = done.ok().flatten();
            },
            done = async { picking.as_mut().unwrap().await }, if picking.is_some() => {
                picking = None;
                if let Ok(mut done) = done {
                    chooser = done.chooser;
                    worker = Some(done.worker);
                    {
                        let mut state = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        if state.pick.as_ref().is_some_and(|pick| pick.request == done.request && pick.token == done.token && pick.account == done.account && pick.channel == done.channel) {
                            let result = publish_batch(&mut state, &mut entries, &mut done.result, limits.unwrap());
                            // Where each came from, for its draft to keep.
                            if let Some(files) = result.as_ref().ok().filter(|files| files.len() == done.origins.len()) {
                                for (file, origin) in files.iter().zip(std::mem::take(&mut done.origins)) {
                                    if let Some(entry) = state.tokens.get_mut(&file.token) {
                                        entry.path = Some(origin);
                                    }
                                }
                            }
                            let pick = state.pick.as_mut().unwrap();
                            pick.result = Some(result);
                            pick.skipped = std::mem::take(&mut done.skipped);
                        }
                    }
                    // A rejected batch can own MiBs. Its last drop is here on
                    // the actor and outside the lightweight UI control lock.
                    drop(done.result);
                    wake();
                }
            },
            job = jobs.recv() => match job {
                None => break,
                Some(job @ Job::Pick { .. }) => {
                    if let Some(limits) = limits.filter(|_| picking.is_none() && saving.is_none() && worker.is_some()) {
                        picking = Some(tokio::spawn(pick_file(chooser.take(), worker.take().unwrap(), job, budget.clone(), limits)));
                    } else if let Job::Pick { request, .. } = job {
                        let mut state = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        if let Some(pick) = state.pick.as_mut().filter(|pick| pick.request == request) { pick.result = Some(Err(UploadError::Busy)); }
                        wake();
                    }
                }
                Some(Job::Send { scope, content, reply_to, mention_reply }) => {
                    if shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner).sends.contains_key(&scope.nonce) {
                        sends.insert(scope.nonce, FrozenSend { content, reply_to, mention_reply });
                        dispatch(&shared, &commands, &entries, &sends, scope, &wake);
                    }
                }
                Some(Job::Retry(scope)) => dispatch(&shared, &commands, &entries, &sends, scope, &wake),
                Some(Job::Post { mut command, tokens }) => {
                    let files = tokens.iter().map(|token| entries.get(token).cloned()).collect::<Option<Vec<_>>>();
                    let mut state = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    let failure = rune_provider::Failure::of(&command);
                    if let (Command::CreatePost { files: carried, .. } | Command::Interact { files: carried, .. }, Some(failure)) = (&mut command, failure) {
                        let error = match files {
                            None => Some(UploadError::NotFound),
                            Some(files) => {
                                *carried = files;
                                commands.try_send(command).err().map(|_| UploadError::Busy)
                            }
                        };
                        if let Some(error) = error {
                            state.post_failures.push((failure, error));
                        }
                    }
                    drop(state);
                    wake();
                }
                // One native dialog at a time: a pick or another save answers Busy.
                Some(job @ Job::Save { .. }) => {
                    if picking.is_none() && saving.is_none() {
                        saving = Some(tokio::spawn(choose_destination(chooser.take(), job)));
                    } else if let Job::Save { reply, .. } = job {
                        let _ = reply.send(Err(UploadError::Busy));
                    }
                }
                #[cfg(test)]
                Some(Job::Insert { account, channel, prepared }) => {
                    let metadata = prepared.metadata().clone();
                    let mut state = shared.lock().unwrap();
                    state.tokens.insert(metadata.token, TokenControl { account, channel, metadata: metadata.clone(), path: None });
                    entries.insert(metadata.token, prepared);
                    wake();
                }
                #[cfg(test)]
                Some(Job::RetainCancelledRead { answer, installed }) => {
                    worker.as_mut().unwrap().retain_cancelled(answer);
                    let _ = installed.send(());
                }
            }
        }
    }
    if let Some(task) = picking {
        task.abort();
    }
    if let Some(task) = saving {
        task.abort();
    }
    // All receivers and payload registries are owned by this runtime task.
}

pub(super) fn publish_batch(
    state: &mut Control,
    entries: &mut HashMap<UploadToken, PreparedUpload>,
    result: &mut prepare::Prepared,
    limits: UploadLimits,
) -> Result<Vec<PendingAttachment>, UploadError> {
    let pick = state.pick.as_ref().ok_or(UploadError::Cancelled)?;
    if *pick.cancel.borrow() || state.account_for(pick.channel).ok() != Some(pick.account) {
        return Err(UploadError::Cancelled);
    }
    let prepared = result.as_ref().map_err(|error| *error)?;
    if prepared.is_empty() || prepared.len() > pick.capacity {
        return Err(UploadError::TooMany);
    }
    let (draft_count, draft_bytes) = state.draft_usage(pick.channel)?;
    if prepared.len()
        > limits
            .max_files
            .min(prepare::MAX_ENTRIES)
            .saturating_sub(draft_count)
        || prepared.len() > prepare::MAX_ENTRIES.saturating_sub(state.tokens.len())
    {
        return Err(UploadError::TooMany);
    }
    let mut total = 0u64;
    for (index, file) in prepared.iter().enumerate() {
        let metadata = file.metadata();
        if pick.token.0.checked_add(index as u64) != Some(metadata.token.0)
            || state.tokens.contains_key(&metadata.token)
            || entries.contains_key(&metadata.token)
        {
            return Err(UploadError::InvalidFile);
        }
        if metadata.size_bytes > limits.max_file_bytes {
            return Err(UploadError::TooLarge);
        }
        total = total
            .checked_add(metadata.size_bytes)
            .ok_or(UploadError::TooLarge)?;
    }
    if total > pick.max_total_bytes
        || total
            .checked_add(draft_bytes)
            .is_none_or(|bytes| bytes > limits.max_message_bytes)
    {
        return Err(UploadError::TooLarge);
    }
    let account = pick.account;
    let channel = pick.channel;
    // Validate the complete batch before moving any payload into the registry.
    // A validation error above leaves the caller's buffers outside the lock.
    let files = std::mem::replace(result, Err(UploadError::Failed))?;
    let mut metadata = Vec::with_capacity(files.len());
    for file in files {
        let summary = file.metadata().clone();
        state.tokens.insert(
            summary.token,
            TokenControl {
                account,
                channel,
                metadata: summary.clone(),
                path: None,
            },
        );
        entries.insert(summary.token, file);
        metadata.push(summary);
    }
    Ok(metadata)
}

fn dispatch(
    shared: &Arc<Mutex<Control>>,
    commands: &mpsc::Sender<Command>,
    entries: &HashMap<UploadToken, PreparedUpload>,
    sends: &HashMap<u64, FrozenSend>,
    scope: UploadSendScope,
    wake: &Arc<dyn Fn() + Send + Sync>,
) {
    let mut state = shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let valid = state.account_for(scope.channel).ok() == Some(scope.account)
        && state.generation == scope.generation;
    let can_reply = state.can_reply(scope.channel);
    let Some(send) = state.sends.get_mut(&scope.nonce) else {
        return;
    };
    if send.scope != scope || send.phase != Phase::Queued {
        return;
    }
    let body = sends.get(&scope.nonce);
    let files = send
        .tokens
        .iter()
        .map(|token| entries.get(token).cloned())
        .collect::<Option<Vec<_>>>();
    let error = if !valid || (send.reply_to.is_some() && !can_reply) {
        Some(UploadError::Unavailable)
    } else if scope.expires_at <= Instant::now() {
        Some(UploadError::Expired)
    } else if body.is_none() || files.is_none() {
        Some(UploadError::NotFound)
    } else {
        None
    };
    if let Some(error) = error {
        Control::fail(send, error, false);
        wake();
        return;
    }
    let body = body.unwrap();
    let command = Command::SendFiles {
        scope,
        content: body.content.clone(),
        reply_to: body.reply_to,
        files: files.unwrap(),
        fence: send.fence.clone(),
        mention_reply: body.mention_reply,
    };
    send.phase = Phase::Waiting;
    if commands.try_send(command).is_err() {
        Control::fail(send, UploadError::Busy, false);
    }
    wake();
}
