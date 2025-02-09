use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use gtk::builders::StackBuilder;
use gtk::gio::MemoryInputStream;
use gtk::glib::{FileError, MainContext};
use gtk::{prelude::*, Box as GtkBox, Button, Entry, Label, Orientation, Stack, StackSwitcher, TextBuffer, TextView, Window, WindowType};
use libipld::Cid;
use reqwest::Client;
use tokio::sync::RwLock;
use webkit2gtk::{NavigationPolicyDecision, NavigationPolicyDecisionExt, PolicyDecisionExt, PolicyDecisionType, SecurityManagerExt, SettingsExt, URIRequestExt, URISchemeRequest, URISchemeRequestExt, WebContext, WebContextExt, WebView, WebViewExt};
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

struct BrowserInner {
    context: WebContext,
    webviews: HashMap<u32, WebView>,
    counter: u32,

    // Widgets
    vbox: GtkBox,
    tab_stack: Stack,
    tab_switcher: StackSwitcher,
}

struct Browser {
    inner: Arc<RwLock<BrowserInner>>,
}

struct SafeBrowser(&'static Browser);
unsafe impl Send for SafeBrowser {}
unsafe impl Sync for SafeBrowser {}
impl AsRef<Browser> for SafeBrowser {
    fn as_ref(&self) -> &Browser {
        self.0
    }
}
impl std::ops::Deref for SafeBrowser {
    type Target = Browser;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl Browser {
    fn new(window: &Window) -> Self {
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

        let vbox = GtkBox::new(Orientation::Horizontal, 0);
        vbox.set_hexpand(true);
        vbox.set_vexpand(true);
        window.add(&vbox);

        let tab_stack = Stack::builder()
            .build();

        let tab_switcher = StackSwitcher::builder()
            .orientation(Orientation::Vertical)
            .stack(&tab_stack)
            .build();
        tab_switcher.set_vexpand(true);
        tab_switcher.set_hexpand(false);

        vbox.pack_start(&tab_switcher, false, false, 0);
        vbox.pack_start(&tab_stack, true, true, 0);

        let inner = BrowserInner {
            context,
            webviews: HashMap::new(),
            counter: 0,
            vbox,
            tab_stack,
            tab_switcher,
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    async fn open_tab(&'static self, uri: &str) {
        let mut browser = self.inner.write().await;

        let id = browser.counter;
        browser.counter += 1;

        let webview = WebView::new_with_context_and_user_content_manager(&browser.context, &UserContentManager::new());
        webview.load_uri(uri);
        webview.set_hexpand(true);
        webview.set_vexpand(true);
        browser.tab_stack.add_named(&webview, &id.to_string());

        let settings = WebViewExt::settings(&webview).unwrap();
        settings.set_enable_developer_extras(true);

        let tab_button = Button::new();
        let tab_button_box = GtkBox::new(Orientation::Horizontal, 0);
        tab_button_box.set_hexpand(true);
        tab_button.add(&tab_button_box);
        let tab_button_label = Label::new(Some(uri));
        tab_button_box.pack_start(&tab_button_label, true, false, 0);
        let tab_button_close = Button::with_label("Close");
        tab_button_box.pack_end(&tab_button_close, false, false, 0);

        webview.connect_title_notify(move |webview| {
            let title = webview.title().unwrap_or_default();
            tab_button_label.set_text(&title);
        });

        webview.connect_decide_policy(move |_, decision, ty| {
            if ty == PolicyDecisionType::NewWindowAction {
                let navigation_decision = decision.clone().downcast::<NavigationPolicyDecision>().unwrap();
                let Some(navigation_action) = navigation_decision.navigation_action() else {return false};
                let Some(request) = navigation_action.request() else {return false};
                let Some(uri) = request.uri() else {return false};
                let uri = uri.to_string();
                let safe_self = SafeBrowser(self);
                tokio::spawn(async move {
                    safe_self.open_tab(&uri)
                });
                decision.ignore();
                true
            } else {
                false
            }
        });

        browser.tab_switcher.add(&tab_button);
        
        browser.webviews.insert(id, webview);
    }
}

#[tokio::main]
async fn main() {
    gtk::init().unwrap();

    let window = Window::new(WindowType::Toplevel);
    window.set_decorated(false);

    let browser = Box::new(Browser::new(&window));
    let mut browser = Box::leak(browser);
    browser.open_tab("ipns://ipfs.tech");
    browser.open_tab("https://google.com");

    // webview.connect_decide_policy(|_, decision, ty| {
    //     println!("{ty:?}");
    //     decision.use_();
    //     true
    // });

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
