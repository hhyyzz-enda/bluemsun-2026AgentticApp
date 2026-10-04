//! Optional preview with sanitized markup and explicit image byte grants.
use makepad_html_renderer::{DocumentSession, HtmlAction, RenderedDocument, RenderOptions, ResourceMap};
use article_core::{assets::crop_cover, document::*, host::Capability};
use super::{model::Grant, storage};
use std::sync::{
    Arc, Mutex, TryLockError,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

static RENDERING: Mutex<()> = Mutex::new(());

fn bundle(
    document: &Document,
    images: &article_makepad::content::Images,
    mut read_asset: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<(String, ResourceMap), String> {
    document.ready()?;
    let mut resources = ResourceMap::default();
    let mut urls = std::collections::BTreeMap::new();
    for id in document.asset_ids() {
        let url = resources
            .insert_image(&id, read_asset(&id)?)
            .map_err(|e| e.to_string())?;
        urls.insert(id, url);
    }
    let (paper, ink, accent) = document.theme.colors();
    let font = if document.large_type { 18 } else { 16 };
    let spacing = if document.compact { "1.55" } else { "1.85" };
    let mut html = format!(
        r#"<html><head><meta charset="utf-8"><style>
html,body{{margin:0;background:#{paper:06x};color:#{ink:06x};font-family:"PingFang SC",system-ui,sans-serif}}
article{{box-sizing:border-box;max-width:760px;margin:0 auto;padding:24px;font-size:{font}px;line-height:{spacing};overflow-wrap:break-word}}
h1{{font-size:28px;line-height:1.4}} h2{{font-size:22px;color:#{accent:06x};margin-top:28px}}
h3{{font-size:19px}} p{{margin:16px 0}} .author,figcaption{{font-size:13px;opacity:.7}}
blockquote{{margin:20px 0;padding:12px 18px;border-left:4px solid #{accent:06x};background:color-mix(in srgb,#{paper:06x} 95%,#{ink:06x})}}
figure{{margin:20px auto;text-align:center}} figure img{{width:100%;max-width:100%;height:auto}} a{{color:#{accent:06x}}} hr{{border:0;border-top:1px solid #{accent:06x};margin:24px 0}}
{}
</style></head><body><article><h1>{}</h1><p class="author">{}</p>"#,
        article_makepad::content::CSS,
        escape(&document.title),
        escape(&document.author)
    );
    if let Some(cover) = &document.cover {
        if cover.show_in_article {
            let bytes = crop_cover(&read_asset(&cover.asset)?, cover, false)?;
            let url = resources
                .insert_image("article-cover.png", bytes)
                .map_err(|e| e.to_string())?;
            html.push_str(&format!(
                "<figure><img src=\"{url}\" alt=\"{}\"></figure>",
                escape(&document.title)
            ));
        }
    }
    for block in &document.blocks {
        if block.kind == BlockKind::Image {
            let url = urls
                .get(block.asset.as_deref().unwrap_or_default())
                .ok_or("Article image is missing")?;
            html.push_str(&format!("<figure style=\"width:{}%\"><img src=\"{url}\" alt=\"{}\"><figcaption>{}</figcaption></figure>", block.width, escape(&block.alt), escape(&block.caption)));
        } else {
            let mut renderer = article_makepad::content::HtmlRenderer {
                images, size: font as f32, ink, error: None, math_count: 0,
                register: |id: &str, png: &[u8]| resources.insert_image(id, png.to_vec()).map_err(|e| e.to_string()),
            };
            let block_html = document.block_html_with_renderer(block, &mut renderer);
            if let Some(error) = renderer.error { return Err(error); }
            html.push_str(&block_html);
        }
    }
    html.push_str("</article></body></html>");
    Ok((html, resources))
}

#[derive(Clone, Debug)]
pub enum Update {
    Rendered(Arc<RenderedDocument>),
    Interaction(HtmlAction),
}

#[derive(Clone, Copy, Debug)]
enum Input {
    Activate(f32, f32),
    Scroll(f32, f32, f32),
}

/// The UI owns this handle, never the DOM or Matrix credentials. Dropping it
/// cancels pending output and disconnects the worker's bounded event queue.
pub struct PreviewSession {
    sender: mpsc::SyncSender<Input>,
    active: Arc<AtomicBool>,
}
impl Drop for PreviewSession {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}
impl PreviewSession {
    pub fn activate(&self, x: f32, y: f32) -> bool {
        self.sender.try_send(Input::Activate(x, y)).is_ok()
    }
    pub fn scroll_horizontal(&self, x: f32, y: f32, delta: f32) -> bool {
        self.sender.try_send(Input::Scroll(x, y, delta)).is_ok()
    }
}

fn guarded<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    // At most one expensive render runs. A backend panic is an error for this
    // disposable session; a later preview must remain usable.
    let _lock = match RENDERING.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return Err("Another article preview is still rendering.".into());
        }
        Err(TryLockError::Poisoned(error)) => error.into_inner(),
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or_else(|_| Err("Unable to render HTML/CSS preview.".into()))
}

fn start_worker(
    build: impl FnOnce() -> Result<DocumentSession, String> + Send + 'static,
    authorize: impl Fn() -> Result<(), String> + Send + 'static,
    emit: impl Fn(Result<Update, String>) + Send + 'static,
) -> PreviewSession {
    let (sender, receiver) = mpsc::sync_channel(4);
    let active = Arc::new(AtomicBool::new(true));
    let alive = active.clone();
    std::thread::spawn(move || {
        let publish = |update| {
            if alive.load(Ordering::Acquire) && authorize().is_ok() {
                emit(update);
            }
        };
        let initial = guarded(|| {
            authorize()?;
            if !alive.load(Ordering::Acquire) {
                return Err("Preview closed".into());
            }
            let mut session = build()?;
            let bitmap = session.render().map_err(|e| e.to_string())?;
            Ok((session, bitmap))
        });
        let mut session = match initial {
            Ok((session, bitmap)) => {
                publish(Ok(Update::Rendered(Arc::new(bitmap))));
                session
            }
            Err(error) => {
                publish(Err(error));
                return;
            }
        };
        while alive.load(Ordering::Acquire) {
            if authorize().is_err() {
                break;
            }
            let input = match receiver.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(input) => input,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            let result = guarded(|| {
                authorize()?;
                if !alive.load(Ordering::Acquire) {
                    return Err("Preview closed".into());
                }
                let action = match input {
                    Input::Activate(x, y) => session.activate(x, y),
                    Input::Scroll(x, y, delta) => {
                        if session.scroll_horizontal(x, y, delta) {
                            HtmlAction::DocumentChanged
                        } else {
                            HtmlAction::None
                        }
                    }
                };
                if action == HtmlAction::DocumentChanged {
                    session
                        .render()
                        .map(|bitmap| Update::Rendered(Arc::new(bitmap)))
                        .map_err(|e| e.to_string())
                } else {
                    Ok(Update::Interaction(action))
                }
            });
            let failed = result.is_err();
            publish(result);
            if failed {
                break;
            } // Never reuse DOM state after a backend failure.
        }
    });
    PreviewSession { sender, active }
}

pub fn start(
    document: Document,
    images: article_makepad::content::Images,
    grant: Grant,
    options: RenderOptions,
    emit: impl Fn(Result<Update, String>) + Send + 'static,
) -> PreviewSession {
    let authority = grant.clone();
    start_worker(
        move || {
            grant.authorize(Capability::ReadDrafts)?;
            let (html, resources) = bundle(&document, &images, |id| {
                storage::asset_bytes(crate::app_data_dir(), &grant, id)
            })?;
            grant.authorize(Capability::ReadDrafts)?;
            DocumentSession::new(&html, options, &resources).map_err(|e| e.to_string())
        },
        move || authority.authorize(Capability::ReadDrafts),
        emit,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    static WORKER_TEST: Mutex<()> = Mutex::new(());

    fn test_document() -> Result<DocumentSession, String> {
        DocumentSession::new(
            "<!doctype html><style>body{margin:0;font:20px/40px Arial}a,summary{display:block}</style><a href='https://example.com/article'>Article link</a><details><summary>Expand</summary><p>Revealed content</p></details>",
            RenderOptions { width_css: 300, scale: 1.0, ..Default::default() },
            &ResourceMap::default(),
        ).map_err(|e| e.to_string())
    }

    #[test]
    fn worker_delivers_links_and_disclosure_rerenders() {
        let _serial = WORKER_TEST.lock().unwrap();
        let (tx, rx) = mpsc::channel();
        let worker = start_worker(
            test_document,
            || Ok(()),
            move |event| {
                let _ = tx.send(event);
            },
        );
        let Update::Rendered(initial) = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
            .unwrap()
        else {
            panic!("initial bitmap missing")
        };
        assert!(worker.activate(20., 20.));
        assert!(
            matches!(rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap().unwrap(), Update::Interaction(HtmlAction::OpenLink { url }) if url == "https://example.com/article")
        );
        assert!(worker.activate(20., 60.));
        let Update::Rendered(expanded) = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
            .unwrap()
        else {
            panic!("disclosure did not rerender")
        };
        assert!(expanded.css_content_height > initial.css_content_height);
        drop(worker);
        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn canceled_and_revoked_workers_cannot_deliver_results() {
        let _serial = WORKER_TEST.lock().unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let worker = start_worker(
            move || {
                entered_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
                test_document()
            },
            || Ok(()),
            move |event| {
                let _ = tx.send(event);
            },
        );
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        drop(worker);
        resume_tx.send(()).unwrap();
        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_secs(10)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));

        let valid = Arc::new(AtomicBool::new(true));
        let authority = valid.clone();
        let (tx, rx) = mpsc::channel();
        let worker = start_worker(
            test_document,
            move || {
                if authority.load(Ordering::Acquire) {
                    Ok(())
                } else {
                    Err("revoked".into())
                }
            },
            move |event| {
                let _ = tx.send(event);
            },
        );
        assert!(
            rx.recv_timeout(std::time::Duration::from_secs(10))
                .unwrap()
                .is_ok()
        );
        valid.store(false, Ordering::Release);
        worker.activate(20., 20.);
        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        drop(worker);
    }

    #[test]
    fn backend_panic_is_reported_and_next_preview_still_works() {
        let _serial = WORKER_TEST.lock().unwrap();
        let (tx, rx) = mpsc::channel();
        let worker = start_worker(
            || panic!("simulated renderer failure"),
            || Ok(()),
            move |event| {
                let _ = tx.send(event);
            },
        );
        assert_eq!(
            rx.recv_timeout(std::time::Duration::from_secs(10))
                .unwrap()
                .unwrap_err(),
            "Unable to render HTML/CSS preview."
        );
        drop(worker);
        let (tx, rx) = mpsc::channel();
        let worker = start_worker(
            test_document,
            || Ok(()),
            move |event| {
                let _ = tx.send(event);
            },
        );
        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_secs(10))
                .unwrap()
                .unwrap(),
            Update::Rendered(_)
        ));
        drop(worker);
        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
    #[test]
    fn editor_md_math_preview_grants_only_generated_images_and_preserves_source() {
        let source = include_str!("../../../lab/article-editor/render-comparison/evidence/09-math/source.md");
        let doc = Document::from_markdown("Math fixture", source).unwrap();
        let before = serde_json::to_string(&doc).unwrap();
        let (html, resources) = bundle(&doc, &Default::default(), |_| panic!("No imported assets expected")).unwrap();
        assert_eq!(html.matches("<img ").count(), 9);
        assert!(!html.contains("<svg") && !html.contains("<math") && !html.contains("<script"));
        assert!(!html.contains("$$") && !html.contains("<pre>"));
        let bitmap = makepad_html_renderer::render_html(&html, RenderOptions {
            width_css:440, scale:2.0, ..Default::default()
        }, &resources).unwrap();
        assert!(bitmap.resources.denied.is_empty());
        assert!(bitmap.resources.served >= 8);
        assert!(!bitmap.clipped);
        assert_eq!(serde_json::to_string(&doc).unwrap(), before);
    }

    #[test]
    fn preview_only_generates_escaped_html_from_validated_document() {
        let mut doc = Document::from_markdown("<script>title</script>", "你好 **世界**").unwrap();
        doc.author = "<img src=file:///private>".into();
        let (html, resources) = bundle(&doc, &Default::default(), |_| panic!("No assets expected")).unwrap();
        assert!(html.contains("&lt;script&gt;title&lt;/script&gt;"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img src=file:"));
        let bitmap =
            makepad_html_renderer::render_html(&html, RenderOptions::default(), &resources)
                .unwrap();
        assert!(bitmap.resources.denied.is_empty());
        assert!(!bitmap.clipped);
    }

    #[test]
    fn small_images_honor_selected_percentage_instead_of_intrinsic_width() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(8, 8, image::Rgba([220, 10, 10, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let bytes = bytes.into_inner();
        let id = blake3::hash(&bytes).to_hex().to_string();
        let mut doc = Document::from_markdown("Image width", "Body").unwrap();
        let mut block = Block::new(BlockKind::Image, "");
        block.asset = Some(id);
        block.width = 50;
        doc.blocks.push(block);
        let options = RenderOptions {
            width_css: 400,
            scale: 1.0,
            ..Default::default()
        };
        let (html, resources) = bundle(&doc, &Default::default(), |_| Ok(bytes.clone())).unwrap();
        let half = makepad_html_renderer::render_html(&html, options, &resources).unwrap();
        doc.blocks.last_mut().unwrap().width = 100;
        let (html, resources) = bundle(&doc, &Default::default(), |_| Ok(bytes.clone())).unwrap();
        let full = makepad_html_renderer::render_html(&html, options, &resources).unwrap();
        assert_eq!(full.resources.denied.len(), 0);
        assert!(full.css_content_height > half.css_content_height + 100.0);
    }
}
