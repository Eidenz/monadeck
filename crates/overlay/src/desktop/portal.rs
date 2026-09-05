//! xdg-desktop-portal ScreenCast negotiation (org.freedesktop.portal.ScreenCast)
//! through `ashpd`, on its own thread with a tiny tokio runtime. Yields the
//! PipeWire node ids of the monitors the user approved and a restore token so
//! the next launch skips the approval dialog.
//!
//! The portal session must stay alive for the streams to keep flowing, so the
//! thread parks on the session until the returned [`Cast`] is dropped.
use std::os::fd::OwnedFd;
use std::sync::mpsc;

use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType, StartCastOptions};
use ashpd::desktop::{CreateSessionOptions, PersistMode};

#[derive(Clone, Debug)]
pub struct StreamInfo {
    pub node_id: u32,
    /// Logical position of the captured monitor in desktop coordinates.
    pub position: Option<(i32, i32)>,
    /// Logical size of the captured monitor.
    pub size: Option<(i32, i32)>,
    /// Connector name (e.g. `DP-3`) when the portal reports one.
    pub mapping_id: Option<String>,
}

pub struct Cast {
    /// PipeWire remote fd from the portal (kept so the connection stays valid;
    /// capture itself uses the user's regular PipeWire socket — the nodes are
    /// visible there for an unsandboxed app, which is how wlx/WayVR do it too).
    pub _fd: OwnedFd,
    pub streams: Vec<StreamInfo>,
    pub restore_token: Option<String>,
    /// Dropping this ends the portal session.
    _keepalive: tokio::sync::oneshot::Sender<()>,
}

/// Kick off a portal request. With `multiple`, the user may tick several
/// monitors in the dialog; each becomes one stream. `token` skips the dialog
/// when it's still valid for this app.
pub fn start(token: Option<String>) -> mpsc::Receiver<Result<Cast, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("screencast-portal".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Err(format!("tokio runtime: {e}")));
                    return;
                }
            };
            rt.block_on(async move {
                let (keep_tx, keep_rx) = tokio::sync::oneshot::channel::<()>();
                match negotiate(token.as_deref()).await {
                    Ok((fd, streams, restore_token, _session)) => {
                        let _ = tx.send(Ok(Cast { _fd: fd, streams, restore_token, _keepalive: keep_tx }));
                        // Park until the Cast is dropped; `_session` lives here.
                        let _ = keep_rx.await;
                        log::info!("screencast: portal session closed");
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
            });
        })
        .expect("spawn portal thread");
    rx
}

type Negotiated = (
    OwnedFd,
    Vec<StreamInfo>,
    Option<String>,
    ashpd::desktop::Session<Screencast>,
);

async fn negotiate(token: Option<&str>) -> Result<Negotiated, String> {
    let proxy = Screencast::new().await.map_err(|e| format!("portal unavailable: {e}"))?;
    let session = proxy
        .create_session(CreateSessionOptions::default())
        .await
        .map_err(|e| format!("CreateSession: {e}"))?;
    let cursor_modes = proxy.available_cursor_modes().await.unwrap_or_default();
    let cursor = if cursor_modes.contains(CursorMode::Embedded) {
        CursorMode::Embedded
    } else {
        CursorMode::Hidden
    };
    let opts = SelectSourcesOptions::default()
        .set_multiple(true)
        .set_cursor_mode(cursor)
        .set_sources(ashpd::enumflags2::BitFlags::from(SourceType::Monitor))
        .set_persist_mode(PersistMode::ExplicitlyRevoked)
        .set_restore_token(token);
    proxy
        .select_sources(&session, opts)
        .await
        .map_err(|e| format!("SelectSources: {e}"))?
        .response()
        .map_err(|e| format!("SelectSources response: {e}"))?;
    let streams = proxy
        .start(&session, None, StartCastOptions::default())
        .await
        .map_err(|e| format!("Start: {e}"))?
        .response()
        .map_err(|e| format!("screen share was not approved: {e}"))?;
    let fd = proxy
        .open_pipe_wire_remote(&session, Default::default())
        .await
        .map_err(|e| format!("OpenPipeWireRemote: {e}"))?;
    let infos: Vec<StreamInfo> = streams
        .streams()
        .iter()
        .map(|s| StreamInfo {
            node_id: s.pipe_wire_node_id(),
            position: s.position(),
            size: s.size(),
            mapping_id: s.mapping_id().map(str::to_owned),
        })
        .collect();
    log::info!("screencast: {} stream(s) approved: {infos:?}", infos.len());
    let restore_token = streams.restore_token().map(str::to_owned);
    Ok((fd, infos, restore_token, session))
}
