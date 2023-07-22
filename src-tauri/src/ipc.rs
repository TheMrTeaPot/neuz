mod bot_config;
mod frontend_info;

pub use self::{
    bot_config::{BotConfig, BotMode, FarmingConfig, ShoutConfig, Slot, SlotType, SupportConfig},
    frontend_info::FrontendInfo,
};
