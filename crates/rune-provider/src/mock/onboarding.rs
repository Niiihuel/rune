//! Rules and onboarding in the simulation (2026-10-18). The club the
//! invitation «club» joins asks both: the account waits on its rules, and
//! its questions give roles and open a channel. The darkroom of
//! «revelado» asks rules with written and chosen answers, which its
//! moderators review; the radio of «radio» has an onboarding a guest is
//! never asked. The owned server is a community whose onboarding the
//! account edits. Everything is fixed, so the rest of the world stays as
//! it was.

use std::collections::HashMap;

use rune_i18n::t;
use rune_model::{
    Channel, ChannelId, ChannelKind, ChoiceId, JoinOutcome, Onboarding, OnboardingMode, Overwrite,
    OverwriteTarget, Permissions, Prompt, PromptChoice, PromptEmoji, PromptId, PromptStyle, Rgb,
    Role, RoleId, RulesAnswer, RulesField, RulesFieldKind, ServerId, ServerRules,
};

use super::{World, ME};
use crate::{Command, Event, Failure};

/// The server the account owns: a community with an onboarding to edit.
const OWNED: ServerId = ServerId(2);

/// What the simulation keeps of each server's rules and onboarding.
#[derive(Debug, Default)]
pub(super) struct OnboardingSim {
    rules: HashMap<ServerId, ServerRules>,
    onboardings: HashMap<ServerId, Onboarding>,
}

fn choice(
    id: u64,
    title: &str,
    emoji: &str,
    roles: Vec<RoleId>,
    channels: Vec<ChannelId>,
) -> PromptChoice {
    PromptChoice {
        id: ChoiceId(id),
        title: title.into(),
        description: None,
        emoji: Some(PromptEmoji {
            name: emoji.into(),
            id: None,
            animated: false,
        }),
        roles,
        channels,
    }
}

fn prompt(
    id: u64,
    title: &str,
    single: bool,
    required: bool,
    options: Vec<PromptChoice>,
) -> Prompt {
    Prompt {
        id: PromptId(id),
        title: title.into(),
        style: PromptStyle::Choices,
        single,
        required,
        in_onboarding: true,
        options,
    }
}

/// The rules every simulated server that screens shows.
fn rules() -> ServerRules {
    ServerRules {
        version: Some("2026-10-18T00:00:00+00:00".into()),
        description: Some("Antes de escribir, lee las reglas del club.".into()),
        fields: vec![RulesField {
            kind: RulesFieldKind::Terms,
            label: "Leí y acepto las reglas".into(),
            description: None,
            placeholder: None,
            rules: vec![
                "Respeta a todas las personas.".into(),
                "Nada de spam ni de autopromoción sin permiso.".into(),
                "Usa cada canal para su tema.".into(),
            ],
            choices: Vec::new(),
            required: true,
            echo: None,
        }],
    }
}

/// The darkroom's rules: agreeing, and three answers its moderators read.
fn reviewed_rules() -> ServerRules {
    let field = |kind, label: &str, required| RulesField {
        kind,
        label: label.into(),
        description: None,
        placeholder: None,
        rules: Vec::new(),
        choices: Vec::new(),
        required,
        echo: None,
    };
    ServerRules {
        version: Some("2026-10-18T12:00:00+00:00".into()),
        description: Some("La moderación lee cada solicitud antes de abrirte el taller.".into()),
        fields: vec![
            RulesField {
                rules: vec![
                    "Cuida los químicos y el equipo.".into(),
                    "Reserva la ampliadora antes de usarla.".into(),
                ],
                ..field(RulesFieldKind::Terms, "Acepto las normas del taller", true)
            },
            RulesField {
                placeholder: Some("Tu nombre".into()),
                ..field(RulesFieldKind::ShortAnswer, "¿Cómo te llamas?", true)
            },
            RulesField {
                choices: vec![
                    "35 mm".into(),
                    "Medio formato".into(),
                    "Gran formato".into(),
                ],
                ..field(RulesFieldKind::MultipleChoice, "¿Qué formato usas?", true)
            },
            field(
                RulesFieldKind::Paragraph,
                "¿Qué te gustaría revelar?",
                false,
            ),
        ],
    }
}

impl World {
    /// The owned server becomes a community with rules and an onboarding,
    /// already finished by the account, which owns it.
    pub(super) fn add_onboarding(&mut self) {
        let channel = |world: &World, name: &str| {
            world
                .channels
                .iter()
                .find(|found| found.server == Some(OWNED) && found.name == name)
                .map(|found| found.id)
        };
        let (Some(core), Some(components), Some(speed)) = (
            channel(self, "limn-core"),
            channel(self, "componentes"),
            channel(self, "rendimiento"),
        ) else {
            return;
        };
        let Some(server) = self.servers.iter_mut().find(|server| server.id == OWNED) else {
            return;
        };
        let options = &mut server.options;
        options.community = true;
        options.media_scan = rune_model::MediaScan::Everyone;
        options.rules_screening = true;
        options.onboarding = true;
        let collaboration = RoleId(OWNED.0 * 10 + 2);
        self.onboarding.rules.insert(OWNED, rules());
        self.onboarding.onboardings.insert(
            OWNED,
            Onboarding {
                enabled: true,
                mode: OnboardingMode::Default,
                default_channels: vec![core, components],
                prompts: vec![prompt(
                    9_100,
                    "¿En qué quieres colaborar?",
                    false,
                    false,
                    vec![
                        choice(
                            9_101,
                            "Componentes",
                            "🧩",
                            vec![collaboration],
                            vec![components],
                        ),
                        choice(9_102, "Rendimiento", "⚡", Vec::new(), vec![speed]),
                    ],
                )],
                chosen: Vec::new(),
            },
        );
        if let Some(member) = self
            .members
            .iter_mut()
            .find(|m| m.server == OWNED && m.user == ME)
        {
            member.restrictions.onboarded = true;
        }
    }

    /// The club joined by invitation asks its rules and its questions:
    /// the account waits on both, with two roles and a channel that only
    /// one of them opens. What changed, as a service reports it.
    pub(super) fn gate_joined(&mut self, server: ServerId) -> Vec<Event> {
        let (photos, music) = (RoleId(self.fresh_id()), RoleId(self.fresh_id()));
        let gallery = ChannelId(self.fresh_id());
        let general = self
            .channels
            .iter()
            .find(|found| found.server == Some(server) && found.kind == ChannelKind::Text)
            .map(|found| found.id);
        let Ok(found) = self.server_mut(server) else {
            return Vec::new();
        };
        let role = |id: RoleId, name: &str, color: u32, position: i32| Role {
            id,
            name: name.into(),
            color: Some(Rgb(color)),
            position,
            hoisted: false,
            mentionable: false,
            permissions: Permissions::NONE,
            style: Default::default(),
        };
        found.roles.push(role(photos, "Fotografía", 0xE5_C0_7B, 2));
        found.roles.push(role(music, "Música", 0x61_AF_EF, 1));
        let options = &mut found.options;
        options.community = true;
        options.media_scan = rune_model::MediaScan::Everyone;
        options.rules_screening = true;
        options.onboarding = true;
        let channel = Channel {
            id: gallery,
            server: Some(server),
            kind: ChannelKind::Text,
            name: "fotos".into(),
            topic: None,
            topic_pictures: Vec::new(),
            parent: None,
            position: 2,
            recipients: Vec::new(),
            owner: None,
            permissions: Permissions::NONE,
            overwrites: vec![
                Overwrite {
                    target: OverwriteTarget::Role(RoleId(server.0)),
                    allow: Permissions::NONE,
                    deny: Permissions::VIEW,
                },
                Overwrite {
                    target: OverwriteTarget::Role(photos),
                    allow: Permissions::VIEW,
                    deny: Permissions::NONE,
                },
            ],
            slowmode_secs: 0,
            nsfw: false,
            icon: None,
            thread: None,
            tags: Vec::new(),
            forum: Default::default(),
        };
        let server_event = Event::ServerChanged(found.clone());
        let ids = (self.fresh_id(), self.fresh_id(), self.fresh_id());
        self.onboarding.rules.insert(server, rules());
        self.onboarding.onboardings.insert(
            server,
            Onboarding {
                enabled: true,
                mode: OnboardingMode::Default,
                default_channels: general.into_iter().collect(),
                prompts: vec![
                    prompt(
                        ids.0,
                        "¿Qué te trae por el club?",
                        false,
                        true,
                        vec![
                            choice(ids.1, "Fotografía", "📷", vec![photos], vec![gallery]),
                            choice(ids.2, "Música", "🎵", vec![music], Vec::new()),
                        ],
                    ),
                    prompt(
                        ids.0 + 100,
                        "¿Cómo prefieres participar?",
                        true,
                        false,
                        vec![
                            choice(ids.2 + 100, "Mirando", "👀", Vec::new(), Vec::new()),
                            choice(ids.2 + 101, "Charlando", "💬", Vec::new(), Vec::new()),
                        ],
                    ),
                ],
                chosen: Vec::new(),
            },
        );
        let Some(member) = self
            .members
            .iter_mut()
            .find(|member| member.server == server && member.user == ME)
        else {
            return vec![server_event];
        };
        member.restrictions.pending = true;
        member.restrictions.onboarded = false;
        let member = member.clone();
        self.channels.push(channel);
        self.sims.insert(gallery, super::invitations::quiet());
        let mut events = vec![server_event, Event::MemberChanged(member)];
        events.extend(self.refresh_permissions(server));
        events
    }

    /// The darkroom joined by invitation asks only its rules, which its
    /// moderators review: the account waits on them.
    pub(super) fn screen_joined(&mut self, server: ServerId) -> Vec<Event> {
        let Ok(found) = self.server_mut(server) else {
            return Vec::new();
        };
        found.options.community = true;
        found.options.rules_screening = true;
        let mut events = vec![Event::ServerChanged(found.clone())];
        self.onboarding.rules.insert(server, reviewed_rules());
        if let Some(member) = self
            .members
            .iter_mut()
            .find(|member| member.server == server && member.user == ME)
        {
            member.restrictions.pending = true;
            events.push(Event::MemberChanged(member.clone()));
        }
        events
    }

    /// The radio a guest joins has an onboarding; the guest is never asked
    /// it, as a service flags such a member.
    pub(super) fn guest_joined(&mut self, server: ServerId) -> Vec<Event> {
        let Ok(found) = self.server_mut(server) else {
            return Vec::new();
        };
        found.options.community = true;
        found.options.onboarding = true;
        let mut events = vec![Event::ServerChanged(found.clone())];
        if let Some(member) = self
            .members
            .iter_mut()
            .find(|member| member.server == server && member.user == ME)
        {
            member.restrictions.guest = true;
            events.push(Event::MemberChanged(member.clone()));
        }
        events
    }

    /// Carry out a command of rules and onboarding, or say why not.
    pub(super) fn onboarding(&mut self, command: Command) -> Vec<Event> {
        let failure = Failure::of(&command).unwrap_or(Failure::Other);
        let outcome = match command {
            Command::LoadServerRules { request, server } => self.server(server).map(|found| {
                let rules = found
                    .options
                    .rules_screening
                    .then(|| self.onboarding.rules.get(&server).cloned())
                    .flatten();
                vec![Event::ServerRules {
                    request,
                    server,
                    rules,
                }]
            }),
            Command::AnswerServerRules {
                request,
                server,
                rules,
                answers,
            } => self.answer_rules(request, server, &rules, &answers),
            Command::LoadOnboarding { request, server } => self.server(server).map(|_| {
                vec![Event::OnboardingLoaded {
                    request,
                    server,
                    onboarding: self.shown_onboarding(server),
                }]
            }),
            Command::AnswerOnboarding {
                request,
                server,
                onboarding,
                chosen,
            } => self.answer_onboarding(request, server, &onboarding, chosen),
            Command::SaveOnboarding {
                request,
                server,
                onboarding,
            } => self.save_onboarding(request, server, onboarding),
            _ => Err(t("la simulación no hace eso").into()),
        };
        outcome.unwrap_or_else(|reason| {
            vec![Event::Failed {
                failure,
                reason,
                ambiguous: false,
            }]
        })
    }

    /// `server`'s onboarding with what the account chose.
    fn shown_onboarding(&self, server: ServerId) -> Option<Onboarding> {
        self.onboarding.onboardings.get(&server).cloned()
    }

    fn answer_rules(
        &mut self,
        request: crate::RequestId,
        server: ServerId,
        shown: &ServerRules,
        answers: &[RulesAnswer],
    ) -> Result<Vec<Event>, String> {
        let current = self
            .onboarding
            .rules
            .get(&server)
            .filter(|_| {
                self.server(server)
                    .is_ok_and(|found| found.options.rules_screening)
            })
            .ok_or(t("este servidor no pide reglas"))?;
        if current != shown {
            return Err(t("las reglas cambiaron: ábrelas otra vez").into());
        }
        if let Some(problem) = current.problem(answers) {
            return Err(problem);
        }
        let reviewed = current.reviewed();
        let member = self
            .members
            .iter_mut()
            .find(|member| member.server == server && member.user == ME)
            .ok_or(t("no eres miembro del servidor"))?;
        if !member.restrictions.pending {
            return Err(t("ya aceptaste las reglas").into());
        }
        let outcome = if reviewed {
            JoinOutcome::InReview
        } else {
            member.restrictions.pending = false;
            JoinOutcome::Approved
        };
        let member = member.clone();
        Ok(vec![
            Event::MemberChanged(member),
            Event::RulesAnswered {
                request,
                server,
                outcome,
            },
        ])
    }

    fn answer_onboarding(
        &mut self,
        request: crate::RequestId,
        server: ServerId,
        shown: &Onboarding,
        chosen: Vec<ChoiceId>,
    ) -> Result<Vec<Event>, String> {
        let current = self
            .onboarding
            .onboardings
            .get_mut(&server)
            .filter(|found| found.asks())
            .ok_or(t("este servidor no tiene preguntas"))?;
        if current.prompts != shown.prompts {
            return Err(t("las preguntas cambiaron: ábrelas otra vez").into());
        }
        if let Some(problem) = current.answer_problem(&chosen) {
            return Err(problem);
        }
        let given: Vec<RoleId> = current
            .asked()
            .flat_map(|prompt| &prompt.options)
            .filter(|option| chosen.contains(&option.id))
            .flat_map(|option| option.roles.iter().copied())
            .collect();
        current.chosen = chosen.clone();
        let member = self
            .members
            .iter_mut()
            .find(|member| member.server == server && member.user == ME)
            .ok_or(t("no eres miembro del servidor"))?;
        for role in given {
            if !member.roles.contains(&role) {
                member.roles.push(role);
            }
        }
        member.restrictions.onboarded = true;
        let member = member.clone();
        let mut events = vec![Event::MemberChanged(member)];
        events.extend(self.refresh_permissions(server));
        events.push(Event::OnboardingAnswered {
            request,
            server,
            chosen,
        });
        Ok(events)
    }

    fn save_onboarding(
        &mut self,
        request: crate::RequestId,
        server: ServerId,
        mut onboarding: Onboarding,
    ) -> Result<Vec<Event>, String> {
        self.require(
            server,
            Permissions::MANAGE_SERVER | Permissions::MANAGE_ROLES,
        )?;
        let found = self.server(server)?;
        if !found.options.community {
            return Err(t("la incorporación es de las comunidades").into());
        }
        let roles: Vec<RoleId> = found.roles.iter().map(|role| role.id).collect();
        let everyone = found.everyone_id();
        let channels: Vec<&Channel> = self
            .channels
            .iter()
            .filter(|channel| channel.server == Some(server))
            .collect();
        let known_channel = |id: &ChannelId| channels.iter().any(|channel| channel.id == *id);
        let options = onboarding.prompts.iter().flat_map(|prompt| &prompt.options);
        if !onboarding.default_channels.iter().all(known_channel)
            || !options.clone().flat_map(|o| &o.channels).all(known_channel)
            || !options
                .flat_map(|o| &o.roles)
                .all(|role| roles.contains(role) && *role != everyone)
        {
            return Err(t("un rol o un canal ya no está en el servidor").into());
        }
        let writes = |id: ChannelId| {
            channels.iter().any(|channel| {
                channel.id == id && rune_model::onboarding::everyone_writes(found, channel)
            })
        };
        if let Some(problem) = onboarding.problem(writes) {
            return Err(problem);
        }
        // What the account chose stays: an edit never sets it.
        onboarding.chosen = self
            .onboarding
            .onboardings
            .get(&server)
            .map(|known| known.chosen.clone())
            .unwrap_or_default();
        let enabled = onboarding.enabled;
        self.onboarding
            .onboardings
            .insert(server, onboarding.clone());
        let found = self.server_mut(server)?;
        found.options.onboarding = enabled;
        Ok(vec![
            Event::ServerChanged(found.clone()),
            Event::OnboardingLoaded {
                request,
                server,
                onboarding: Some(onboarding),
            },
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockConfig;
    use crate::InvitationDestination;

    fn joined() -> (World, ServerId) {
        let mut world = World::build(&MockConfig::default());
        let target =
            super::super::invitations::target(&format!("{}club", super::super::invitations::LINKS))
                .unwrap();
        let destination = world.invitation(&target).unwrap().destination;
        let InvitationDestination::Server(server) = destination else {
            panic!("the club is a server");
        };
        world.accept_invitation(1, &target, destination);
        (world, server)
    }

    fn mine(world: &World, server: ServerId) -> rune_model::Member {
        world.member(server, ME).unwrap().clone()
    }

    #[test]
    fn the_club_waits_on_its_rules_and_lets_the_account_in_once_accepted() {
        let (mut world, server) = joined();
        assert!(mine(&world, server).restrictions.pending);
        let events = world.onboarding(Command::LoadServerRules { request: 2, server });
        let [Event::ServerRules {
            rules: Some(rules), ..
        }] = events.as_slice()
        else {
            panic!("{events:?}")
        };
        let refused = world.onboarding(Command::AnswerServerRules {
            request: 3,
            server,
            rules: rules.clone(),
            answers: vec![RulesAnswer::Agreed(false)],
        });
        assert!(matches!(
            refused.as_slice(),
            [Event::Failed {
                failure: Failure::Rules { request: 3, .. },
                ..
            }]
        ));
        assert!(mine(&world, server).restrictions.pending);
        let events = world.onboarding(Command::AnswerServerRules {
            request: 4,
            server,
            rules: rules.clone(),
            answers: vec![RulesAnswer::Agreed(true)],
        });
        assert!(matches!(
            events.last(),
            Some(Event::RulesAnswered {
                outcome: JoinOutcome::Approved,
                ..
            })
        ));
        assert!(!mine(&world, server).restrictions.pending);
        // Once is enough.
        let again = world.onboarding(Command::AnswerServerRules {
            request: 5,
            server,
            rules: rules.clone(),
            answers: vec![RulesAnswer::Agreed(true)],
        });
        assert!(matches!(again.as_slice(), [Event::Failed { .. }]));
    }

    #[test]
    fn answering_the_questions_gives_their_roles_and_opens_their_channel() {
        let (mut world, server) = joined();
        let events = world.onboarding(Command::LoadOnboarding { request: 2, server });
        let [Event::OnboardingLoaded {
            onboarding: Some(onboarding),
            ..
        }] = events.as_slice()
        else {
            panic!("{events:?}")
        };
        assert!(onboarding.asks() && !mine(&world, server).restrictions.onboarded);
        let photos = onboarding.prompts[0].options[0].clone();
        let gallery = photos.channels[0];
        assert!(!world
            .channel(gallery)
            .unwrap()
            .permissions
            .contains(Permissions::VIEW));
        // The required question first.
        let refused = world.onboarding(Command::AnswerOnboarding {
            request: 3,
            server,
            onboarding: onboarding.clone(),
            chosen: Vec::new(),
        });
        assert!(matches!(refused.as_slice(), [Event::Failed { .. }]));
        let events = world.onboarding(Command::AnswerOnboarding {
            request: 4,
            server,
            onboarding: onboarding.clone(),
            chosen: vec![photos.id],
        });
        assert!(
            matches!(events.last(), Some(Event::OnboardingAnswered { chosen, .. }) if *chosen == vec![photos.id])
        );
        assert!(events
            .iter()
            .any(|event| matches!(event, Event::ChannelChanged(channel) if channel.id == gallery)));
        let member = mine(&world, server);
        assert!(member.restrictions.onboarded && member.roles == photos.roles);
        assert!(world
            .channel(gallery)
            .unwrap()
            .permissions
            .contains(Permissions::VIEW));
    }

    #[test]
    fn the_owner_saves_the_onboarding_of_its_community_within_bounds() {
        let mut world = World::build(&MockConfig::default());
        let loaded = world.onboarding(Command::LoadOnboarding {
            request: 1,
            server: OWNED,
        });
        let [Event::OnboardingLoaded {
            onboarding: Some(onboarding),
            ..
        }] = loaded.as_slice()
        else {
            panic!("the owned server has one: {loaded:?}");
        };
        let mut onboarding = onboarding.clone();
        assert!(mine(&world, OWNED).restrictions.onboarded);
        onboarding.prompts[0].title = "¿Qué te interesa?".into();
        onboarding.mode = OnboardingMode::Advanced;
        let events = world.onboarding(Command::SaveOnboarding {
            request: 2,
            server: OWNED,
            onboarding: onboarding.clone(),
        });
        assert!(
            matches!(events.last(), Some(Event::OnboardingLoaded { request: 2, onboarding: Some(saved), .. }) if *saved == onboarding)
        );
        // Without a channel everyone writes in, it does not turn on.
        onboarding.default_channels.clear();
        onboarding.mode = OnboardingMode::Default;
        let refused = world.onboarding(Command::SaveOnboarding {
            request: 3,
            server: OWNED,
            onboarding: onboarding.clone(),
        });
        assert!(matches!(
            refused.as_slice(),
            [Event::Failed {
                failure: Failure::OnboardingEdit { request: 3, .. },
                ..
            }]
        ));
        // Off, it saves, and the server says so.
        onboarding.enabled = false;
        let events = world.onboarding(Command::SaveOnboarding {
            request: 4,
            server: OWNED,
            onboarding,
        });
        assert!(matches!(&events[0], Event::ServerChanged(server) if !server.options.onboarding));
        // Where the account does not manage the server, nothing changes.
        let refused = world.onboarding(Command::SaveOnboarding {
            request: 5,
            server: ServerId(1),
            onboarding: Onboarding::default(),
        });
        assert!(matches!(refused.as_slice(), [Event::Failed { .. }]));
    }
}
