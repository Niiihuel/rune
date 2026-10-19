use super::*;
use base64::Engine as _;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn pdf() -> Vec<u8> {
    // Two owned pages: the first solid red, the second blue. No external fonts/resources.
    pdf_of(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 4 0 R >>".into(),
        "<< /Length 26 >>\nstream\n1 0 0 rg 0 0 240 120 re f\nendstream".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Contents 6 0 R >>".into(),
        "<< /Length 26 >>\nstream\n0 0 1 rg 0 0 240 120 re f\nendstream".into(),
    ])
}

/// Two owned pages of text in a standard font, for a search; the second
/// one shown a quarter turn clockwise (`/Rotate 90`).
fn text_pdf() -> Vec<u8> {
    let stream = |said: &str| {
        let content = format!("BT /F1 18 Tf 20 60 Td ({said}) Tj ET");
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        )
    };
    let page = |contents: u32, rotate: u32| {
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 120] /Rotate {rotate} /Resources << /Font << /F1 7 0 R >> >> /Contents {contents} 0 R >>")
    };
    pdf_of(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".into(),
        page(4, 0),
        stream("Hola taller"),
        page(6, 90),
        stream("Segunda pagina"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ])
}

fn pdf_of(objects: &[String]) -> Vec<u8> {
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    let size = objects.len() + 1;
    bytes.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    bytes
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).await.unwrap(), 1);
        request.push(byte[0]);
        assert!(request.len() <= 4096);
    }
    String::from_utf8(request).unwrap()
}

async fn server(status: &str, extra: &str, body: Vec<u8>) -> (url::Url, JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let response = format!(
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n{extra}\r\n",
        body.len()
    );
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_request(&mut socket).await;
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.write_all(&body).await;
        request
    });
    (
        url::Url::parse(&format!("http://{address}/owned.pdf")).unwrap(),
        task,
    )
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

#[tokio::test]
async fn pdf_download_is_explicit_bounded_and_never_sends_account_headers() {
    let document = pdf();
    let size = document.len() as u64;
    let (url, request) = server(
        "200 OK",
        "Content-Type: application/pdf\r\n",
        document.clone(),
    )
    .await;
    assert_eq!(download(&client(), url, size).await.unwrap(), document);
    let request = request.await.unwrap().to_ascii_lowercase();
    assert!(!request.contains("authorization:") && !request.contains("cookie:"));
    assert_eq!(
        download(
            &client(),
            url::Url::parse("https://example.invalid").unwrap(),
            MAX_DOCUMENT_BYTES + 1
        )
        .await
        .unwrap_err(),
        DocumentError::TooLarge
    );
    for (status, extra, body, expected, error) in [
        (
            "302 Found",
            "Location: https://example.invalid/forbidden\r\n",
            Vec::new(),
            0,
            DocumentError::Failed,
        ),
        (
            "200 OK",
            "",
            b"not a PDF".to_vec(),
            0,
            DocumentError::Invalid,
        ),
        (
            "200 OK",
            "",
            document.clone(),
            size + 1,
            DocumentError::Invalid,
        ),
        (
            "200 OK",
            "",
            vec![0; MAX_DOCUMENT_BYTES as usize + 1],
            0,
            DocumentError::TooLarge,
        ),
    ] {
        let (url, task) = server(status, extra, body).await;
        assert_eq!(download(&client(), url, expected).await.unwrap_err(), error);
        task.await.unwrap();
    }
}

fn completed(epoch: u64, request: u64, document: Option<Arc<[u8]>>) -> Completed {
    Completed {
        epoch,
        result: DocumentResult {
            request,
            page: 1,
            pages: 3,
            result: Err(DocumentError::Failed),
            marks: Vec::new(),
        },
        document,
    }
}

#[tokio::test]
async fn cancelled_and_old_epoch_results_never_reopen_a_document() {
    let mut service = Documents::new(Handle::current(), &[], Arc::new(|| {}));
    let old = service.epoch;
    service.clear();
    service
        .sender
        .send(completed(old, 4, Some(Arc::from(&b"%PDF-"[..]))))
        .await
        .unwrap();
    assert!(service.poll().is_none());
    assert!(service.document.is_none(), "an old download is not kept");
    service
        .sender
        .send(completed(service.epoch, 5, None))
        .await
        .unwrap();
    assert_eq!(service.poll().unwrap().request, 5);
}

#[tokio::test]
async fn later_pages_need_the_open_document_and_closing_frees_it() {
    let mut service = Documents::new(Handle::current(), &[], Arc::new(|| {}));
    assert_eq!(
        service.page(5, 2, Vec::new()),
        Err(DocumentError::Failed),
        "nothing is open"
    );
    service
        .sender
        .send(completed(
            service.epoch,
            5,
            Some(Arc::from(&b"%PDF-1.4"[..])),
        ))
        .await
        .unwrap();
    let first = service.poll().unwrap();
    assert_eq!((first.page, first.pages), (1, 3));
    assert!(service.document.is_some(), "kept in memory for its pages");
    assert_eq!(
        service.page(6, 2, Vec::new()),
        Err(DocumentError::Invalid),
        "another request"
    );
    assert_eq!(service.page(5, 0, Vec::new()), Err(DocumentError::Invalid));
    assert_eq!(
        service.page(5, 4, Vec::new()),
        Err(DocumentError::Invalid),
        "past the last page"
    );
    service.cancel(4);
    assert!(service.document.is_some(), "another request's cancel");
    service.cancel(5);
    assert!(
        service.document.is_none(),
        "closing the viewer frees the PDF"
    );
    assert_eq!(service.page(5, 2, Vec::new()), Err(DocumentError::Failed));
}

#[tokio::test]
async fn a_new_page_replaces_the_one_still_rendering() {
    let mut service = Documents::new(Handle::current(), &[], Arc::new(|| {}));
    service.document = Some((5, Arc::from(&b"%PDF-invalid"[..]), 3));
    if render::Tools::configured().is_none() {
        assert_eq!(
            service.page(5, 2, Vec::new()),
            Err(DocumentError::Unavailable)
        );
        return;
    }
    // This test runtime runs nothing until the test awaits: the first page's
    // task is aborted before its sandbox starts.
    service.page(5, 2, Vec::new()).unwrap();
    service.page(5, 3, vec![[0.0, 0.0, 1.0, 1.0]]).unwrap();
    let result = loop {
        if let Some(result) = service.poll() {
            break result;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(result.page, 3);
    assert_eq!(result.marks, [[0.0, 0.0, 1.0, 1.0]], "said as asked");
    assert!(result.result.is_err(), "an invalid document has no page");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(service.poll().is_none(), "page 2 never arrives");
    assert!(
        service.document.is_some(),
        "a failed page keeps the document"
    );
}

#[tokio::test]
async fn chunked_documents_cannot_bypass_the_actual_byte_limit() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let chunk = vec![b'x'; 65_536];
        // One extra full chunk exceeds 16 MiB without a Content-Length header.
        for _ in 0..=MAX_DOCUMENT_BYTES / chunk.len() as u64 {
            if socket.write_all(b"10000\r\n").await.is_err()
                || socket.write_all(&chunk).await.is_err()
                || socket.write_all(b"\r\n").await.is_err()
            {
                break;
            }
        }
        let _ = socket.write_all(b"0\r\n\r\n").await;
    });
    let url = url::Url::parse(&format!("http://{address}/owned.pdf")).unwrap();
    assert_eq!(
        download(&client(), url, 0).await.unwrap_err(),
        DocumentError::TooLarge
    );
    server.await.unwrap();
}

#[tokio::test]
#[ignore = "requires nix-shell --arg withDocs true and Linux user namespaces"]
async fn sandbox_counts_and_renders_only_the_requested_owned_page() {
    let tools = render::Tools::configured().expect("build withDocs=true");
    let document: Arc<[u8]> = pdf().into();
    let result = render::page(document.clone(), 1, tools, Vec::new())
        .await
        .unwrap();
    assert_eq!((result.width, result.height), (1200, 600));
    assert!(result.size_bytes < 8 * 1024 * 1024);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(result.base64.as_bytes())
        .unwrap();
    let raster = image::load_from_memory(&bytes).unwrap().to_rgb8();
    assert_eq!(
        raster.get_pixel(600, 300).0,
        [255, 0, 0],
        "only the first page is red"
    );
    assert!(matches!(
        render::page(Arc::from(&b"%PDF-invalid"[..]), 1, tools, Vec::new()).await,
        Err(DocumentError::Failed)
    ));
    assert_eq!(render::page_count(document.clone(), tools).await, Some(2));
    assert_eq!(
        render::page_count(Arc::from(&b"%PDF-invalid"[..]), tools).await,
        None
    );
    let second = render::page(document.clone(), 2, tools, Vec::new())
        .await
        .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(second.base64.as_bytes())
        .unwrap();
    let raster = image::load_from_memory(&bytes).unwrap().to_rgb8();
    assert_eq!(
        raster.get_pixel(600, 300).0,
        [0, 0, 255],
        "the second page is blue"
    );
    assert!(matches!(
        render::page(document, 3, tools, Vec::new()).await,
        Err(DocumentError::Failed)
    ));
    println!("Owned two-page PDF: 2 pages counted; page 1 red and page 2 blue at 1200x600; page 3 and invalid input rejected; no user document used.");
}

#[tokio::test]
#[ignore = "requires nix-shell --arg withDocs true and Linux user namespaces"]
async fn sandbox_reads_the_text_of_each_page_for_a_search() {
    let tools = render::Tools::configured().expect("build withDocs=true");
    let document: Arc<[u8]> = text_pdf().into();
    let pages = render::text(document.clone(), tools).await.unwrap();
    assert_eq!(pages.len(), 2, "{pages:?}");
    fn words(page: &[rune_model::PdfWord]) -> Vec<&str> {
        page.iter().map(|word| word.text.as_str()).collect()
    }
    assert_eq!(words(&pages[0]), ["Hola", "taller"]);
    assert_eq!(words(&pages[1]), ["Segunda", "pagina"]);
    // 18 pt at 20, 60 from the bottom of a 240×120 page.
    let [left, top, right, bottom] = pages[0][0].bounds;
    assert!((left - 20.0 / 240.0).abs() < 0.01, "{left}");
    assert!(
        top > 0.3 && bottom < 0.6 && right > left,
        "{:?}",
        pages[0][0]
    );
    // Highlighted where it says, on the page as rendered.
    let marked = render::page(document, 1, tools, vec![pages[0][1].bounds])
        .await
        .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(marked.base64.as_bytes())
        .unwrap();
    let raster = image::load_from_memory(&bytes).unwrap().to_rgb8();
    let [left, top, right, bottom] = pages[0][1].bounds;
    let tinted = raster
        .enumerate_pixels()
        .filter(|(_, _, pixel)| pixel.0 == [255, 214, 64]);
    let (mut inside, mut outside) = (0, 0);
    for (x, y, _) in tinted {
        let (x, y) = (x as f32 / 1200.0, y as f32 / 600.0);
        if (left - 0.01..=right + 0.01).contains(&x) && (top - 0.01..=bottom + 0.01).contains(&y) {
            inside += 1;
        } else {
            outside += 1;
        }
    }
    assert!(
        inside > 1000 && outside == 0,
        "{inside} tinted in the word, {outside} out"
    );
    // On the turned page, 120 wide and 240 tall as shown, the word that
    // starts 20 pt in runs down from 20 pt under the top.
    let [left, top, right, bottom] = pages[1][0].bounds;
    assert!((top - 20.0 / 240.0).abs() < 0.01, "{:?}", pages[1][0]);
    assert!(
        left > 0.4 && right < 0.7 && bottom > top,
        "{:?}",
        pages[1][0]
    );
    let turned = render::page(text_pdf().into(), 2, tools, vec![pages[1][0].bounds])
        .await
        .unwrap();
    assert_eq!((turned.width, turned.height), (600, 1200));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(turned.base64.as_bytes())
        .unwrap();
    let raster = image::load_from_memory(&bytes).unwrap().to_rgb8();
    // Under the highlight there is ink of the word, not blank paper.
    let under = raster
        .enumerate_pixels()
        .filter(|(x, y, _)| {
            let (x, y) = (*x as f32 / 600.0, *y as f32 / 1200.0);
            (left..right).contains(&x) && (top..bottom).contains(&y)
        })
        .filter(|(_, _, pixel)| pixel.0[2] < 32 && pixel.0[0] < 200)
        .count();
    assert!(
        under > 100,
        "{under} dark pixels of the word under its highlight"
    );
    // Pictures alone read as pages without words, not as a failure.
    let colors = render::text(pdf().into(), tools).await.unwrap();
    assert_eq!(colors.len(), 2);
    assert!(colors.iter().all(Vec::is_empty));
    assert!(render::text(Arc::from(&b"%PDF-invalid"[..]), tools)
        .await
        .is_err());
    println!("Owned two-page text PDF: each page's words and their places read in the sandbox and highlighted on the rendered page; pages of pictures read empty; invalid input rejected.");
}

#[tokio::test]
async fn text_needs_the_open_document_and_closing_drops_it() {
    let runtime = Handle::current();
    let mut service = Documents::new(runtime, &[], Arc::new(|| {}));
    assert_eq!(service.text(5), Err(DocumentError::Failed), "nothing open");
    service.document = Some((5, Arc::from(&b"%PDF-1.4"[..]), 2));
    assert_eq!(
        service.text(6),
        Err(DocumentError::Invalid),
        "another request"
    );
    service.clear();
    assert!(service.poll_text().is_none());
}
