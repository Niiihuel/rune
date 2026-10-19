use super::*;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn entry(id: &str) -> Value {
    json!({"type":"gif", "id":id, "title":"Saludo", "username":"creator",
        "url":format!("https://giphy.com/gifs/saludo-{id}"),
        "images":{"fixed_width_small_still":{"url":format!("https://media2.giphy.com/media/{id}/100w_s.gif?cid=fixture&rid=100w_s.gif")}}})
}

fn page(offset: u32, count: usize, total: usize) -> Value {
    json!({"data":(0..count).map(|index| entry(&format!("id{}", offset as usize+index))).collect::<Vec<_>>(),
        "meta":{"status":200},"pagination":{"offset":offset,"count":count,"total_count":total}})
}

fn service(origin: &str) -> GifSearch {
    let mut service = GifSearch::new(
        Handle::current(),
        Some(GiphyConfig::new("fixture-key".into()).unwrap()),
        Arc::new(|| {}),
    );
    let api = service.api.as_mut().unwrap();
    api.base = url::Url::parse(&format!("{origin}/v1/gifs/")).unwrap();
    api.thumbnails = false;
    service
}

async fn request(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        assert!(bytes.len() < 8192);
        bytes.push(socket.read_u8().await.unwrap());
    }
    String::from_utf8(bytes).unwrap()
}

async fn reply(socket: &mut tokio::net::TcpStream, body: &[u8]) {
    socket
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    socket.write_all(body).await.unwrap();
}

async fn result(service: &mut GifSearch) -> GifResult {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(result) = service.poll() {
                return result;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn missing_configuration_is_explicit_and_secret_debug_is_redacted() {
    let mut service = GifSearch::new(Handle::current(), None, Arc::new(|| {}));
    assert!(!service.available());
    service.start(1, GifQuery::Trending, 0);
    assert_eq!(
        result(&mut service).await.result,
        Err(GifError::NotConfigured)
    );
    let config = GiphyConfig::new("fixture-key".into()).unwrap();
    assert!(!format!("{config:?}").contains("fixture-key"));
    assert!(!format!("{:?}", GifQuery::Search("private search".into())).contains("private search"));
    for key in ["", "bad key", "bad\nkey", "bad?key"] {
        assert!(GiphyConfig::new(key.into()).is_err());
    }
}

#[tokio::test]
async fn documented_search_and_pagination_keep_queries_urls_and_order_without_account_headers() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut service = service(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move {
        for (offset, route) in [
            (0, "/v1/gifs/search"),
            (20, "/v1/gifs/search"),
            (0, "/v1/gifs/trending"),
        ] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = request(&mut socket).await;
            let headers = request.to_ascii_lowercase();
            assert!(!headers.contains("authorization:") && !headers.contains("cookie:"));
            let target = request
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            let url = url::Url::parse(&format!("http://fixture.invalid{target}")).unwrap();
            assert_eq!(url.path(), route);
            let pairs = url
                .query_pairs()
                .into_owned()
                .collect::<std::collections::HashMap<_, _>>();
            assert_eq!(pairs["api_key"], "fixture-key");
            assert_eq!(pairs["offset"], offset.to_string());
            assert_eq!(pairs["rating"], "pg-13");
            if route.ends_with("search") {
                assert_eq!(pairs["q"], "@art café & hola");
            } else {
                assert!(!pairs.contains_key("q"));
            }
            reply(&mut socket, page(offset, 20, 44).to_string().as_bytes()).await;
        }
    });
    let query = GifQuery::Search("@art café & hola".into());
    service.start(1, query.clone(), 0);
    let first = result(&mut service).await.result.unwrap();
    assert_eq!(first.next_offset, Some(20));
    assert_eq!(first.items[0].id, "id0");
    assert_eq!(first.items[19].id, "id19");
    assert_eq!(
        first.items[0].preview.url,
        "https://media2.giphy.com/media/id0/100w_s.gif?cid=fixture&rid=100w_s.gif"
    );
    assert_eq!(first.attribution.label, "Powered By GIPHY");
    service.start(2, query, 20);
    assert_eq!(
        result(&mut service).await.result.unwrap().items[0].id,
        "id20"
    );
    service.start(3, GifQuery::Trending, 0);
    assert_eq!(result(&mut service).await.request, 3);
    server.await.unwrap();
}

#[test]
fn invalid_descriptors_reject_the_page_instead_of_filtering_reordering_or_fetching_any_origin() {
    let valid = page(0, 2, 2);
    assert!(decode(valid.to_string().as_bytes(), 0)
        .unwrap()
        .next_offset
        .is_none());
    for invalid in [
        "http://media.giphy.com/x.gif",
        "https://media.giphy.com.evil.test/x.gif",
        "https://media.giphy.com:444/x.gif",
        "https://user:secret@media.giphy.com/x.gif",
        "https://127.0.0.1/x.gif",
        "https://media.giphy.com/x.gif#fragment",
    ] {
        let mut data = valid.clone();
        data["data"][1]["images"]["fixed_width_small_still"]["url"] = json!(invalid);
        assert_eq!(
            decode(data.to_string().as_bytes(), 0),
            Err(GifError::InvalidResponse)
        );
    }
    for field in ["url", "id", "title"] {
        let mut data = valid.clone();
        data["data"][0][field] = json!("bad\nvalue");
        assert!(decode(data.to_string().as_bytes(), 0).is_err());
    }
    let mut duplicate = valid.clone();
    duplicate["data"][1] = duplicate["data"][0].clone();
    assert!(decode(duplicate.to_string().as_bytes(), 0).is_err());
    assert!(decode(page(20, 20, 50).to_string().as_bytes(), 0).is_err());
    assert!(decode(page(0, 21, 50).to_string().as_bytes(), 0).is_err());
    assert!(!valid_query(&GifQuery::Search("x".repeat(51)), 0));
    assert!(!valid_query(&GifQuery::Trending, MAX_OFFSET + 1));
}

#[tokio::test]
async fn cancel_and_new_queries_discard_old_work_and_old_queued_results() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut service = service(&format!("http://{}", listener.local_addr().unwrap()));
    service.start(1, GifQuery::Search("old".into()), 0);
    let (mut old, _) = listener.accept().await.unwrap();
    request(&mut old).await;
    let old_epoch = service.epoch;
    service.start(2, GifQuery::Search("new".into()), 0);
    assert!(tokio::time::timeout(Duration::from_secs(1), old.read_u8())
        .await
        .unwrap()
        .is_err());
    service
        .sender
        .send((
            old_epoch,
            GifResult {
                request: 1,
                query: GifQuery::Search("old".into()),
                offset: 0,
                result: Err(GifError::Unavailable),
            },
        ))
        .await
        .unwrap();
    assert!(service.poll().is_none());
    let (mut current, _) = listener.accept().await.unwrap();
    request(&mut current).await;
    reply(&mut current, page(0, 1, 1).to_string().as_bytes()).await;
    assert_eq!(result(&mut service).await.request, 2);
    service.start(3, GifQuery::Trending, 0);
    let (mut socket, _) = listener.accept().await.unwrap();
    request(&mut socket).await;
    service.cancel(3);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), socket.read_u8())
            .await
            .unwrap()
            .is_err()
    );
    assert!(service.poll().is_none());
}

#[tokio::test]
async fn rate_limit_survives_cancellation_before_poll_and_reconfiguration() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let mut service = service(&origin);
    service.start(1, GifQuery::Trending, 0);
    let (mut socket, _) = listener.accept().await.unwrap();
    request(&mut socket).await;
    socket
        .write_all(
            b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 0\r\n\r\n",
        )
        .await
        .unwrap();
    while service.receiver.is_empty() {
        tokio::task::yield_now().await;
    }
    service.clear();
    service.configure(Some(GiphyConfig::new("fixture-new-key".into()).unwrap()));
    service.api.as_mut().unwrap().base = url::Url::parse(&format!("{origin}/v1/gifs/")).unwrap();
    service.start(2, GifQuery::Trending, 0);
    assert!(matches!(
        result(&mut service).await.result,
        Err(GifError::RateLimited {
            retry_after_secs: 119..=121
        })
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn redirects_and_oversized_bodies_are_not_followed_or_exposed() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut service = service(&format!("http://{}", listener.local_addr().unwrap()));
    let target = trap.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        socket.write_all(format!("HTTP/1.1 302 Found\r\nConnection: close\r\nLocation: http://{target}/trap\r\nContent-Length: 0\r\n\r\n").as_bytes()).await.unwrap();
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
                    RESPONSE_BYTES + 1
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    service.start(1, GifQuery::Trending, 0);
    assert_eq!(
        result(&mut service).await.result,
        Err(GifError::Unavailable)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), trap.accept())
            .await
            .is_err()
    );
    service.start(2, GifQuery::Trending, 0);
    assert_eq!(
        result(&mut service).await.result,
        Err(GifError::InvalidResponse)
    );
    server.await.unwrap();
}

#[tokio::test]
async fn page_thumbnails_have_four_slots_and_normalize_without_an_artwork_cache() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let mut service = service(&origin);
    service.api.as_mut().unwrap().media_origin = Some(origin.clone());
    let mut jobs = JoinSet::new();
    for index in 0..8 {
        let (api, url) = (
            service.api.as_ref().unwrap().clone(),
            format!("{origin}/{index}.gif"),
        );
        jobs.spawn(async move {
            api.thumbnail(RemoteImage {
                url,
                kind: ImageKind::Artwork,
            })
            .await
        });
    }
    let mut sockets = Vec::new();
    for _ in 0..THUMBNAIL_CONCURRENT {
        let (mut socket, _) = listener.accept().await.unwrap();
        let headers = request(&mut socket).await.to_lowercase();
        assert!(
            !headers.contains("api_key")
                && !headers.contains("authorization:")
                && !headers.contains("cookie:")
        );
        sockets.push(socket);
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    let bytes = [
        b'G', b'I', b'F', b'8', b'9', b'a', 1, 0, 1, 0, 0x80, 0, 0, 0, 0, 0, 255, 255, 255, 0x2c,
        0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 1, 0, 0x3b,
    ];
    for mut socket in sockets {
        reply(&mut socket, &bytes).await;
    }
    for _ in 0..4 {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        reply(&mut socket, &bytes).await;
    }
    let mut count = 0;
    while let Some(result) = jobs.join_next().await {
        let thumbnail = result.unwrap().unwrap();
        assert_eq!((thumbnail.width, thumbnail.height), (128, 128));
        assert!(thumbnail.png_base64.len() <= 128 * 128 * 8);
        count += 1;
    }
    assert_eq!(count, 8);
}

#[tokio::test]
async fn slow_thumbnail_deadline_keeps_the_valid_page_and_cancels_its_media_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let mut service = service(&origin);
    let api = service.api.as_mut().unwrap();
    api.thumbnails = true;
    api.media_origin = Some(origin);
    api.thumbnail_budget = Duration::from_millis(100);
    service.start(1, GifQuery::Trending, 0);
    let (mut socket, _) = listener.accept().await.unwrap();
    request(&mut socket).await;
    reply(&mut socket, page(0, 20, 40).to_string().as_bytes()).await;
    let mut media = Vec::new();
    for _ in 0..4 {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        media.push(socket);
    }
    let page = result(&mut service).await.result.unwrap();
    assert_eq!(page.items.len(), 20);
    assert_eq!(page.items[0].id, "id0");
    assert_eq!(page.items[19].id, "id19");
    assert_eq!(page.next_offset, Some(20));
    assert!(page.items.iter().all(|item| item.thumbnail.is_none()));
    for mut socket in media {
        assert!(
            tokio::time::timeout(Duration::from_secs(1), socket.read_u8())
                .await
                .unwrap()
                .is_err()
        );
    }
}
