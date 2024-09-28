use gtk::{prelude::*, Entry, Box, Orientation, Window, WindowType};
use webkit2gtk::{SettingsExt, URISchemeRequest, URISchemeRequestExt, WebContext, WebContextExt, WebView, WebViewExt};
use webkit2gtk::{UserContentManager, WebViewExtManual};
use webkit2gtk::{glib, gio};

fn serve_ipfs(request: &URISchemeRequest) {
    println!("test");
    let uri = request.uri().unwrap();
    let hash = &uri[7..];
    let webview = request.web_view().unwrap();
    webview.load_uri(format!("https://gateway.ipfs.io/ipfs/{}", hash).as_str());
}

fn main() {
    gtk::init().unwrap();

    let window = Window::new(WindowType::Toplevel);
    let context = WebContext::default().unwrap();
    context.set_web_extensions_initialization_user_data(&"webkit".to_variant());
    context.set_web_extensions_directory("../webkit2gtk-webextension-rs/example/target/debug/");
    context.register_uri_scheme("ipfs", serve_ipfs);
    let webview = WebView::new_with_context_and_user_content_manager(&context, &UserContentManager::new());
    webview.load_uri("ipfs://QmbsGZ999Xk757uSGFMbFW2xW4F21CbGvmpx8A5JwP5Y5s");

    let vbox = Box::new(Orientation::Vertical, 0);
    vbox.set_hexpand(true);
    vbox.set_vexpand(true);
    
    let url_bar = Entry::new();
    url_bar.set_text(webview.uri().unwrap().as_str());
    url_bar.set_hexpand(true);
    vbox.pack_start(&url_bar, false, false, 0);
    
    webview.set_hexpand(true);
    webview.set_vexpand(true);
    vbox.pack_start(&webview, true, true, 0);
    
    window.add(&vbox);

    let settings = WebViewExt::settings(&webview).unwrap();
    settings.set_enable_developer_extras(true);

    /*let inspector = webview.get_inspector().unwrap();
    inspector.show();*/

    window.show_all();

    //webview.evaluate_javascript("alert('Hello');", None, None, None::<&gio::Cancellable>, |_result| {});
    // webview.evaluate_javascript("42", None, None, None, |result| match result {
    //     Ok(result) => {
    //         let value = result.js_value().unwrap();
    //         println!("is_boolean: {}", value.is_boolean());
    //         println!("is_number: {}", value.is_number());
    //         println!("{:?}", value.to_int32());
    //         println!("{:?}", value.to_boolean());
    //     }
    //     Err(error) => println!("{}", error),
    // });

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    
    let webview2 = webview.clone();
    let url_bar2 = url_bar.clone();
    url_bar.connect_activate(move |_| {
        let url = url_bar2.text();
        webview2.load_uri(&url);
    });

    webview.connect_uri_notify(move |webview| {
        let uri = webview.uri().unwrap();
        url_bar.set_text(uri.as_str());
    });

    gtk::main();
}
