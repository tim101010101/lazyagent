use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use tracing::debug;

use crate::protocol::{AgentSession, Provider};
use crate::tmux::TmuxController;

pub enum BgRequest {
    Refresh,
    Capture { pane_id: String },
    Shutdown,
}

pub enum BgUpdate {
    Sessions(Vec<AgentSession>),
    Preview { pane_id: String, content: String },
}

pub fn spawn_worker(
    providers: Vec<Box<dyn Provider>>,
) -> (Sender<BgRequest>, Receiver<BgUpdate>, JoinHandle<()>) {
    let (req_tx, req_rx) = mpsc::channel::<BgRequest>();
    let (upd_tx, upd_rx) = mpsc::channel::<BgUpdate>();

    let handle = thread::spawn(move || {
        while let Ok(req) = req_rx.recv() {
            match req {
                BgRequest::Refresh => {
                    debug!("bg: refresh requested");
                    let sessions = TmuxController::discover_sessions(&providers);
                    debug!(count = sessions.len(), "bg: refresh complete");
                    if upd_tx.send(BgUpdate::Sessions(sessions)).is_err() {
                        break;
                    }
                }
                BgRequest::Capture { pane_id } => {
                    debug!(pane_id = %pane_id, "bg: capture requested");
                    if let Some(content) = TmuxController::capture_pane(&pane_id) {
                        if upd_tx
                            .send(BgUpdate::Preview { pane_id, content })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                BgRequest::Shutdown => break,
            }
        }
    });

    (req_tx, upd_rx, handle)
}
