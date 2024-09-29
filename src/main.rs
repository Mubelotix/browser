use std::sync::LazyLock;
use gtk::builders::StackBuilder;
use gtk::gio::MemoryInputStream;
use gtk::glib::{FileError, MainContext};
use gtk::{prelude::*, Box, Entry, Orientation, Stack, StackSwitcher, TextBuffer, TextView, Window, WindowType};
use libipld::Cid;
use reqwest::Client;
use webkit2gtk::{PolicyDecisionExt, SecurityManagerExt, SettingsExt, URISchemeRequest, URISchemeRequestExt, WebContext, WebContextExt, WebView, WebViewExt};
use webkit2gtk::{UserContentManager, WebViewExtManual};
use webkit2gtk::{glib, gio};

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::new()
});

// The guys from Tauri do it so I guess it's safe
// https://github.com/tauri-apps/wry/blob/eafaadb9b35f74f07ef83da3d3e738c9ae631f7c/src/webkitgtk/web_context.rs#L359
struct UriSchemeRequestSafe(URISchemeRequest);
unsafe impl Send for UriSchemeRequestSafe {}
unsafe impl Sync for UriSchemeRequestSafe {}

impl UriSchemeRequestSafe {
    fn finish_error(&self, error: &mut webkit2gtk::Error) {
        self.0.finish_error(error);
    }

    fn finish(&self, stream: &impl IsA<gio::InputStream>, stream_length: i64, content_type: Option<&str>) {
        self.0.finish(stream, stream_length, content_type);
    }
}

async fn fetch_url(url: String) -> Result<(glib::Bytes, i64, Option<String>), webkit2gtk::Error> {
    // TODO: Pass useful headers like range, etc.
            
    let response = CLIENT.get(url).send().await;
    let response = match response {
        Ok(response) => response,
        Err(error) => return Err(webkit2gtk::Error::new(FileError::Failed, &format!("{error}: {error:?}"))),
    };

    // TODO: Someday we might be able to implement PollableInputStream and use streams
    // let stream = response.bytes_stream();

    let content_type = response.headers().get("content-type").and_then(|value| value.to_str().ok()).map(|s| s.to_string());
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => return Err(webkit2gtk::Error::new(FileError::Failed, &format!("{error}: {error:?}"))),
    };
    let len = bytes.len() as i64;

    Ok((glib::Bytes::from_owned(bytes), len, content_type))
}

fn serve_other_url(request: &URISchemeRequest, url: String) {
    let request = UriSchemeRequestSafe(request.clone());
    tokio::spawn(async move {
        let result = fetch_url(url).await;
        MainContext::default().invoke(move || {
            match result {
                Ok((bytes, len, content_type)) => {
                    let input_stream = MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
                    request.finish(&input_stream, len, content_type.as_deref());
                }
                Err(error) => request.finish_error(&mut error.clone()),
            }
        });
    });   
}

fn serve_ipfs(request: &URISchemeRequest) {
    let uri = request.uri().unwrap_or_default();
    let value = match uri.starts_with("ipfs://") {
        true => uri[7..].to_string(),
        false => uri.to_string(),
    };
    let (cid, path) = value.split_once('/').unwrap_or((value.as_str(), ""));
    let cid = match Cid::try_from(cid) {
        Ok(cid) => cid,
        Err(error) => {
            let mut webkit_error = webkit2gtk::Error::new(FileError::Failed, &format!("{error}: {error:?}"));
            request.finish_error(&mut webkit_error);
            return;
        }
    };
    let cid = match cid.into_v1() {
        Ok(cid_v1) => cid_v1,
        Err(error) => {
            let mut webkit_error = webkit2gtk::Error::new(FileError::Failed, &format!("{error}: {error:?}"));
            request.finish_error(&mut webkit_error);
            return;
        }
    };

    serve_other_url(request, format!("http://{cid}.ipfs.localhost:8080/{path}"));
}

fn serve_ipns(request: &URISchemeRequest) {
    let uri = request.uri().unwrap_or_default();
    let value = match uri.starts_with("ipns://") {
        true => uri[7..].to_string(),
        false => uri.to_string(),
    };
    let (domain, path) = value.split_once('/').unwrap_or((value.as_str(), ""));
    serve_other_url(request, format!("http://{domain}.ipns.localhost:8080/{path}"));
}

#[tokio::main]
async fn main() {
    gtk::init().unwrap();

    let window = Window::new(WindowType::Toplevel);
    window.set_decorated(false);

    let context = WebContext::default().unwrap();
    context.set_web_extensions_initialization_user_data(&"webkit".to_variant());
    context.set_web_extensions_directory("../webkit2gtk-webextension-rs/example/target/debug/");
    context.register_uri_scheme("ipfs", serve_ipfs);
    context.register_uri_scheme("ipns", serve_ipns);
    let security_manager = context.security_manager().unwrap();
    security_manager.register_uri_scheme_as_secure("ipfs");
    security_manager.register_uri_scheme_as_secure("ipns");
    security_manager.register_uri_scheme_as_cors_enabled("ipfs");
    security_manager.register_uri_scheme_as_cors_enabled("ipns");
    let webview = WebView::new_with_context_and_user_content_manager(&context, &UserContentManager::new());
    webview.load_uri("ipns://ipfs.tech");

    // webview.connect_decide_policy(|_, decision, ty| {
    //     println!("{ty:?}");
    //     decision.use_();
    //     true
    // });


    let vbox = Box::new(Orientation::Horizontal, 0);
    vbox.set_hexpand(true);
    vbox.set_vexpand(true);
    window.add(&vbox);

    let tab_stack = Stack::builder()
        .child(&webview)
        .build();

    let tab_name = "Tab 1";
    tab_stack.set_visible_child_name(tab_name);

    let tab_name_text_buffer = TextBuffer::builder()
        .text(tab_name)
        .build();

    let tab_name_widget = TextView::builder()
        .buffer(&tab_name_text_buffer)
        .build();

    let tab_switcher = StackSwitcher::builder()
        .stack(&tab_stack)
        .child(&tab_name_widget)
        .build();
    vbox.pack_start(&tab_switcher, false, false, 0);
    
    webview.set_hexpand(true);
    webview.set_vexpand(true);
    vbox.pack_start(&tab_stack, true, true, 0);

    let settings = WebViewExt::settings(&webview).unwrap();
    settings.set_enable_developer_extras(true);

    /*let inspector = webview.get_inspector().unwrap();
    inspector.show();*/
    
    // let webview2 = webview.clone();
    // let url_bar2 = url_bar.clone();
    // url_bar.connect_activate(move |_| {
    //     let url = url_bar2.text();
    //     webview2.load_uri(&url);
    // });

    // webview.connect_uri_notify(move |webview| {
    //     let uri = webview.uri().unwrap();
    //     url_bar.set_text(uri.as_str());
    // });

    // Get the screen size
    let display = window.display();
    let monitor = display.primary_monitor().unwrap_or_else(|| display.monitor(0).unwrap());
    let geometry = monitor.geometry();
    let width = geometry.width();
    let height = geometry.height();

    // Set the window size to the screen size
    window.set_default_size(width, height);
    window.show_all();

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });

    gtk::main();
}
