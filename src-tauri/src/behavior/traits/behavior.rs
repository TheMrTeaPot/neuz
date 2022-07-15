use crate::{image_analyzer::ImageAnalyzer, ipc::BotConfig, platform::PlatformAccessor};

pub trait Behavior<'a> {
    fn new(platform: &'a PlatformAccessor<'a>) -> Self;
    fn start(&mut self);
    fn stop(&mut self);
    fn run_iteration(&mut self, config: &BotConfig, analyzer: ImageAnalyzer);
}