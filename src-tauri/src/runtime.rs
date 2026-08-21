use crate::types::RuntimeEvent;
use tauri::{AppHandle, Emitter};

pub(crate) trait RuntimeEventSink: Send + Sync {
    fn emit(&self, event: RuntimeEvent);
    /// 供需要窗口控制的工具（如内置浏览器）获取 AppHandle；headless 返回 None。
    fn app_handle(&self) -> Option<&AppHandle> {
        None
    }
}

pub(crate) struct TauriRuntimeEventSink {
    app: AppHandle,
}

impl TauriRuntimeEventSink {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl RuntimeEventSink for TauriRuntimeEventSink {
    fn emit(&self, event: RuntimeEvent) {
        let _ = self.app.emit("kf://runtime", event);
    }

    fn app_handle(&self) -> Option<&AppHandle> {
        Some(&self.app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<RuntimeEvent>>,
    }

    impl RuntimeEventSink for RecordingSink {
        fn emit(&self, event: RuntimeEvent) {
            self.events.lock().push(event);
        }
    }

    #[test]
    fn recording_sink_preserves_emit_order() {
        let sink = RecordingSink::default();
        sink.emit(RuntimeEvent::new("first", serde_json::json!({})));
        sink.emit(RuntimeEvent::new("second", serde_json::json!({})));
        let kinds: Vec<_> = sink
            .events
            .lock()
            .iter()
            .map(|event| event.kind.clone())
            .collect();
        assert_eq!(kinds, ["first", "second"]);
    }
}
