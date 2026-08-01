// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DesktopOperation {
    Bootstrapping,
    InspectingPoint,
    EstimatingDownload,
    Downloading,
    ReadingCache,
    Calculating,
    DeletingCache,
    Exporting,
    ConfiguringBasemap,
}

impl DesktopOperation {
    fn label(self) -> &'static str {
        match self {
            Self::Bootstrapping => "应用初始化",
            Self::InspectingPoint => "区域数据检查",
            Self::EstimatingDownload => "数据下载量检查",
            Self::Downloading => "区域数据下载",
            Self::ReadingCache => "缓存状态读取",
            Self::Calculating => "传播计算",
            Self::DeletingCache => "缓存删除",
            Self::Exporting => "结果导出",
            Self::ConfiguringBasemap => "在线地图配置",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum CancellationTarget {
    Calculation,
    Download,
}

impl CancellationTarget {
    fn matches(self, operation: DesktopOperation) -> bool {
        match self {
            Self::Calculation => operation == DesktopOperation::Calculating,
            Self::Download => matches!(
                operation,
                DesktopOperation::EstimatingDownload | DesktopOperation::Downloading
            ),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct DesktopOperationController {
    state: Arc<Mutex<DesktopOperationState>>,
}

#[derive(Default)]
struct DesktopOperationState {
    next_id: u64,
    active: Option<ActiveDesktopOperation>,
}

struct ActiveDesktopOperation {
    id: u64,
    operation: DesktopOperation,
    cancelled: Arc<AtomicBool>,
}

pub(crate) struct DesktopOperationLease {
    id: u64,
    operation: DesktopOperation,
    cancelled: Arc<AtomicBool>,
    state: Arc<Mutex<DesktopOperationState>>,
    finished: bool,
}

impl DesktopOperationController {
    pub(crate) fn begin(
        &self,
        operation: DesktopOperation,
    ) -> Result<DesktopOperationLease, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "operation state lock is poisoned".to_string())?;
        if let Some(current) = &state.active {
            return Err(format!(
                "{}正在进行，请稍候或先取消",
                current.operation.label()
            ));
        }
        let id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        state.active = Some(ActiveDesktopOperation {
            id,
            operation,
            cancelled: Arc::clone(&cancelled),
        });
        Ok(DesktopOperationLease {
            id,
            operation,
            cancelled,
            state: Arc::clone(&self.state),
            finished: false,
        })
    }

    pub(crate) fn cancel(&self, target: CancellationTarget) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        let Some(active) = &state.active else {
            return false;
        };
        if !target.matches(active.operation) {
            return false;
        }
        active.cancelled.store(true, Ordering::Release);
        true
    }
}

impl DesktopOperationLease {
    pub(crate) fn cancellation_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    pub(crate) fn finish<T>(mut self, outcome: Result<T, String>) -> Result<T, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "operation state lock is poisoned".to_string())?;
        let is_current = state
            .active
            .as_ref()
            .is_some_and(|active| active.id == self.id && active.operation == self.operation);
        if !is_current {
            return Err("operation lease lost its active identity".into());
        }
        let was_cancelled = self.cancelled.load(Ordering::Acquire);
        state.active = None;
        self.finished = true;
        drop(state);
        if was_cancelled {
            Err("操作已取消".into())
        } else {
            outcome
        }
    }
}

impl Drop for DesktopOperationLease {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            let is_current = state
                .active
                .as_ref()
                .is_some_and(|active| active.id == self.id && active.operation == self.operation);
            if is_current {
                state.active = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_before_finish_discards_success() {
        let controller = DesktopOperationController::default();
        let lease = controller.begin(DesktopOperation::Calculating).unwrap();
        let cancelled = lease.cancellation_flag();
        assert!(controller.cancel(CancellationTarget::Calculation));
        assert!(cancelled.load(Ordering::Acquire));
        assert!(lease.finish(Ok(42)).is_err());
        assert!(!controller.cancel(CancellationTarget::Calculation));
    }

    #[test]
    fn finish_before_cancel_keeps_success() {
        let controller = DesktopOperationController::default();
        let lease = controller.begin(DesktopOperation::Calculating).unwrap();
        assert_eq!(lease.finish(Ok(42)).unwrap(), 42);
        assert!(!controller.cancel(CancellationTarget::Calculation));
    }

    #[test]
    fn download_cancel_only_matches_download_family() {
        let controller = DesktopOperationController::default();
        let lease = controller.begin(DesktopOperation::Downloading).unwrap();
        assert!(!controller.cancel(CancellationTarget::Calculation));
        assert!(controller.cancel(CancellationTarget::Download));
        assert!(lease.finish(Ok(())).is_err());
        let estimate = controller
            .begin(DesktopOperation::EstimatingDownload)
            .unwrap();
        assert!(controller.cancel(CancellationTarget::Download));
        assert!(estimate.finish(Ok(())).is_err());
    }

    #[test]
    fn every_desktop_operation_has_a_user_facing_label() {
        let operations = [
            DesktopOperation::Bootstrapping,
            DesktopOperation::InspectingPoint,
            DesktopOperation::EstimatingDownload,
            DesktopOperation::Downloading,
            DesktopOperation::ReadingCache,
            DesktopOperation::Calculating,
            DesktopOperation::DeletingCache,
            DesktopOperation::Exporting,
            DesktopOperation::ConfiguringBasemap,
        ];
        assert!(
            operations
                .into_iter()
                .all(|operation| !operation.label().is_empty())
        );
    }
}
