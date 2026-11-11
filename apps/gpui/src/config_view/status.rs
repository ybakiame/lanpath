#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PreviewStatus {
    Idle,
    Generating,
    Ready,
    Failed,
}

impl PreviewStatus {
    pub(super) fn start_block_reason(self) -> Option<&'static str> {
        match self {
            Self::Idle => Some("正在准备配置预览"),
            Self::Generating => Some("正在生成并校验配置…"),
            Self::Ready => None,
            Self::Failed => Some("配置生成失败，请在配置预览中查看原因"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PreviewStatus;

    #[test]
    fn generation_blocks_start_without_requesting_manual_configuration() {
        assert_eq!(PreviewStatus::Ready.start_block_reason(), None);
        assert_eq!(
            PreviewStatus::Generating.start_block_reason(),
            Some("正在生成并校验配置…")
        );
        assert_ne!(
            PreviewStatus::Generating.start_block_reason(),
            PreviewStatus::Failed.start_block_reason()
        );
        assert!(PreviewStatus::Idle.start_block_reason().is_some());
        assert!(PreviewStatus::Failed.start_block_reason().is_some());
    }
}
