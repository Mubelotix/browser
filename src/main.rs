use gtk::{prelude::*, Window, WindowType};
use webkit2gtk::{SettingsExt, WebContext, WebContextExt, WebView, WebViewExt};
use webkit2gtk::{UserContentManager, WebViewExtManual};
use webkit2gtk::{glib, gio};

fn main() {
    gtk::init().unwrap();

    let window = Window::new(WindowType::Toplevel);
    let context = WebContext::default().unwrap();
    context.set_web_extensions_initialization_user_data(&"webkit".to_variant());
    context.set_web_extensions_directory("../webkit2gtk-webextension-rs/example/target/debug/");
    let webview =
        WebView::new_with_context_and_user_content_manager(&context, &UserContentManager::new());
    webview.load_uri("https://crates.io/");
    window.add(&webview);

    let settings = WebViewExt::settings(&webview).unwrap();
    settings.set_enable_developer_extras(true);

    /*let inspector = webview.get_inspector().unwrap();
    inspector.show();*/

    window.show_all();

    webview.evaluate_javascript("alert('Hello');", None, None, None::<&gio::Cancellable>, |_result| {});
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

    gtk::main();
}
