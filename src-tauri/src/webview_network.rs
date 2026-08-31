use std::sync::Arc;

use tauri::WebviewWindow;

use crate::{
    adblock::AdBlocker,
    webview_bridge::{
        anime_cover_url, cover_update, is_aniworld_episode_page, is_aniworld_page, playback_update,
        update_install_request, window_action_request, NetworkHandlers, PageContext,
    },
};

#[cfg(windows)]
pub fn install_network_filter(
    window: &WebviewWindow,
    blocker: Arc<AdBlocker>,
    page_context: PageContext,
    handlers: NetworkHandlers,
) -> tauri::Result<()> {
    window.with_webview(move |platform_webview| {
        if let Err(error) = unsafe {
            install_webview2_network_filter(platform_webview, blocker, page_context, handlers)
        } {
            eprintln!("Could not enable the WebView2 ad blocker: {error}");
        }
    })
}

#[cfg(windows)]
unsafe fn install_webview2_network_filter(
    platform_webview: tauri::webview::PlatformWebview,
    blocker: Arc<AdBlocker>,
    page_context: PageContext,
    handlers: NetworkHandlers,
) -> windows::core::Result<()> {
    use webview2_com::{
        take_pwstr, Microsoft::Web::WebView2::Win32::*, WebResourceRequestedEventHandler,
    };
    use windows::core::{Interface, HSTRING, PWSTR};

    let webview = platform_webview.controller().CoreWebView2()?;
    let environment = platform_webview.environment();
    let filter = HSTRING::from("*");

    if let Ok(webview_22) = webview.cast::<ICoreWebView2_22>() {
        webview_22.AddWebResourceRequestedFilterWithRequestSourceKinds(
            &filter,
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
            COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
        )?;
    } else {
        webview.AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;
    }

    let mut registration_token = 0_i64;
    webview.add_WebResourceRequested(
        &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };

            let request = args.Request()?;
            let url = {
                let mut value = PWSTR::null();
                request.Uri(&mut value)?;
                take_pwstr(value)
            };
            if let Some(candidate) = cover_update(&url) {
                let source_url = page_context.current_url();
                if let Some(cover_url) = anime_cover_url(&candidate, &source_url) {
                    (handlers.cover)(source_url, cover_url);
                }
                let status = HSTRING::from("No Content");
                let headers =
                    HSTRING::from("Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n");
                let response =
                    environment.CreateWebResourceResponse(None, 204, &status, &headers)?;
                args.SetResponse(&response)?;
                return Ok(());
            }
            if let Some((playing, seeking, position_ms, duration_ms, rate_milli)) =
                playback_update(&url)
            {
                let source_url = page_context.current_url();
                if is_aniworld_episode_page(&source_url) {
                    (handlers.playback)(playing, seeking, position_ms, duration_ms, rate_milli);
                }
                let status = HSTRING::from("No Content");
                let headers =
                    HSTRING::from("Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n");
                let response =
                    environment.CreateWebResourceResponse(None, 204, &status, &headers)?;
                args.SetResponse(&response)?;
                return Ok(());
            }
            if update_install_request(&url) {
                let source_url = page_context.current_url();
                if is_aniworld_page(&source_url) {
                    (handlers.update)();
                }
                let status = HSTRING::from("No Content");
                let headers =
                    HSTRING::from("Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n");
                let response =
                    environment.CreateWebResourceResponse(None, 204, &status, &headers)?;
                args.SetResponse(&response)?;
                return Ok(());
            }
            if let Some(action) = window_action_request(&url) {
                let source_url = page_context.current_url();
                if is_aniworld_page(&source_url) {
                    (handlers.window)(action);
                }
                let status = HSTRING::from("No Content");
                let headers =
                    HSTRING::from("Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n");
                let response =
                    environment.CreateWebResourceResponse(None, 204, &status, &headers)?;
                args.SetResponse(&response)?;
                return Ok(());
            }
            let method = {
                let mut value = PWSTR::null();
                request.Method(&mut value)?;
                take_pwstr(value)
            };
            let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_OTHER;
            args.ResourceContext(&mut context)?;

            let source_url = page_context.current_url();
            if let Some(cover_url) = anime_cover_url(&url, &source_url) {
                (handlers.cover)(source_url.clone(), cover_url);
            }
            let request_type = webview2_request_type(context);
            if blocker.blocks(&url, &source_url, request_type, &method) {
                let status = HSTRING::from("No Content");
                let headers = HSTRING::from("Cache-Control: no-store\r\n");
                let response =
                    environment.CreateWebResourceResponse(None, 204, &status, &headers)?;
                args.SetResponse(&response)?;
            }

            Ok(())
        })),
        &mut registration_token,
    )?;

    Ok(())
}

#[cfg(windows)]
fn webview2_request_type(
    context: webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_RESOURCE_CONTEXT,
) -> &'static str {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;

    match context {
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT => "document",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET => "stylesheet",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE => "image",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA => "media",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT => "font",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT => "script",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST => "xmlhttprequest",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH => "fetch",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_PING => "ping",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_WEBSOCKET => "websocket",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_CSP_VIOLATION_REPORT => "csp_report",
        _ => "other",
    }
}

#[cfg(not(windows))]
pub fn install_network_filter(
    _window: &WebviewWindow,
    _blocker: Arc<AdBlocker>,
    _page_context: PageContext,
    _handlers: NetworkHandlers,
) -> tauri::Result<()> {
    Ok(())
}
