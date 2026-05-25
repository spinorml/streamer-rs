/*
 * SpinorML Ltd 🚀 AGPL-3.0 License - https://spinorml.com/license
 */

use crate::error::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineState {
    Null,
    Ready,
    Paused,
    Playing,
}

#[async_trait]
pub trait Pipeline: Send {
    fn build(description: &str) -> Result<Self>
    where
        Self: Sized;
    async fn play(&mut self) -> Result<()>;
    async fn pause(&mut self) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    fn state(&self) -> PipelineState;
}
