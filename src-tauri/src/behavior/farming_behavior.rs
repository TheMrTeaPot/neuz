use std::time::{Duration, Instant};

use rand::prelude::SliceRandom;
use slog::Logger;
use tauri::{Window, Manager};

use crate::{
    data::{Bounds, MobType, Point, Target, TargetType},
    image_analyzer::ImageAnalyzer,
    ipc::{BotConfig, FarmingConfig, FrontendInfo, SlotType},
    movement::MovementAccessor,
    play,
    utils::DateTime, platform::KeyManager,
};

use super::{Behavior, SlotsUsage};

#[derive(Debug, Clone, Copy)]
enum State {
    NoEnemyFound,
    SearchingForEnemy,
    EnemyFound(Target),
    Attacking(Target),
    AfterEnemyKill(Target),
}

pub struct FarmingBehavior<'a> {
    rng: rand::rngs::ThreadRng,
    logger: &'a Logger,
    movement: &'a MovementAccessor,
    state: State,
    slots_usage: SlotsUsage<'a>,
    key_manager: &'a KeyManager,
    last_initial_attack_time: Instant,
    last_kill_time: Instant,
    avoided_bounds: Vec<(Bounds, Instant, u128)>,
    rotation_movement_tries: u32,
    is_attacking: bool,
    kill_count: u32,
    obstacle_avoidance_count: u32,
    last_summon_pet_time: Option<Instant>,
    last_killed_type: MobType,
    start_time: Instant,
    already_attack_count: u32,
    last_click_pos: Option<Point>,
    stealed_target_count: u32,
    last_no_ennemy_time: Option<Instant>,
}

impl<'a> Behavior<'a> for FarmingBehavior<'a> {
    fn new(logger: &'a Logger, movement: &'a MovementAccessor, key_manager: &'a KeyManager) -> Self {
        Self {
            logger,
            movement,
            rng: rand::thread_rng(),
            state: State::SearchingForEnemy,
            key_manager,
            slots_usage: SlotsUsage::new(key_manager, "Farming".to_string()),
            last_initial_attack_time: Instant::now(),
            last_kill_time: Instant::now(),
            avoided_bounds: vec![],
            is_attacking: false,
            rotation_movement_tries: 0,
            kill_count: 0,
            obstacle_avoidance_count: 0,
            last_summon_pet_time: None,
            last_killed_type: MobType::Passive,
            start_time: Instant::now(),
            already_attack_count: 0,
            last_click_pos: None,
            stealed_target_count: 0,
            last_no_ennemy_time: None,
        }
    }

    fn start(&mut self, config: &BotConfig) {
        self.slots_usage.update_config(config.clone());
    }

    fn update(&mut self, config: &BotConfig) {
        self.slots_usage.update_config(config.clone());
    }

    fn stop(&mut self, _config: &BotConfig) {}

    fn run_iteration(
        &mut self,
        frontend_info: &mut FrontendInfo,
        config: &BotConfig,
        image: &mut ImageAnalyzer,
    ) {
        let bot_config = config;
        let config = bot_config.farming_config();
        // Update all needed timestamps
        self.update_timestamps(config);

        // Check whether something should be restored
        self.slots_usage.check_restorations(image);

        // Send chat message if there's
        self.slots_usage.get_slot_for(None, SlotType::ChatMessage, true);

        // Check state machine
        self.state = match self.state {
            State::NoEnemyFound => self.on_no_enemy_found(bot_config),
            State::SearchingForEnemy => self.on_searching_for_enemy(bot_config, config, image),
            State::EnemyFound(mob) => self.on_enemy_found(bot_config, image, mob, frontend_info),
            State::Attacking(mob) => self.on_attacking(bot_config, config, mob, image),
            State::AfterEnemyKill(_) => self.after_enemy_kill(frontend_info),
        };

        frontend_info.set_is_attacking(self.is_attacking);
    }
}

impl<'a> FarmingBehavior<'_> {
    fn update_timestamps(&mut self, config: &FarmingConfig) {
        self.update_pickup_pet(config);

        self.slots_usage.update_slots_usage();

        self.update_avoid_bounds();
    }

    /// Update avoid bounds cooldowns timers
    fn update_avoid_bounds(&mut self) {
        let mut result: Vec<(Bounds, Instant, u128)> = vec![];
        for n in 0..self.avoided_bounds.len() {
            let current = self.avoided_bounds[n];
            if current.1.elapsed().as_millis() < current.2 {
                result.push(current);
            }
        }
        self.avoided_bounds = result;
    }

    /// Check whether pickup pet should be unsummoned
    fn update_pickup_pet(&mut self, config: &FarmingConfig) {
        if let Some(pickup_pet_slot_index) = config.slot_index(SlotType::PickupPet) {
            if let Some(last_time) = self.last_summon_pet_time {
                if last_time.elapsed().as_millis()
                    > config
                        .get_slot_cooldown(pickup_pet_slot_index.0, pickup_pet_slot_index.1)
                        .unwrap_or(3000) as u128
                {
                    self.key_manager.send_slot_eval(
                        pickup_pet_slot_index.0,
                        pickup_pet_slot_index.1,
                    );
                    self.last_summon_pet_time = None;
                }
            }
        }
    }

    /// Pickup items on the ground.
    fn pickup_items(&mut self) {
        let slot = self.slots_usage.get_slot_for(None, SlotType::PickupPet, false);
        if slot.is_some() {
            let index = slot.unwrap();
            if self.last_summon_pet_time.is_none() {
                self.key_manager.send_slot_eval( index.0, index.1);
                self.last_summon_pet_time = Some(Instant::now());
            } else {
                // if pet is already out, just reset it's timer
                self.last_summon_pet_time = Some(Instant::now());
            }
        } else {
            let slot = self.slots_usage.get_slot_for(None, SlotType::PickupMotion, false);
            if slot.is_some() {
                let index = slot.unwrap();

                for _i in 1..7 {
                    self.key_manager.send_slot_eval( index.0, index.1);
                }
            }
        }
    }

    fn on_no_enemy_found(&mut self, config: &BotConfig) -> State {
        if let Some (last_no_ennemy_time) = self.last_no_ennemy_time {
            if config.inactivity_timeout() > 0 && last_no_ennemy_time.elapsed().as_millis() > config.inactivity_timeout() {
                self.key_manager.handle.exit(0);
            }
        }else {
            self.last_no_ennemy_time = Some(Instant::now());
        }
        use crate::movement::prelude::*;
        // Try rotating first in order to locate nearby enemies
        if self.rotation_movement_tries < 30 {
            play!(self.movement => [
                // Rotate in random direction for a random duration
                Rotate(rot::Right, dur::Fixed(50)),
                // Wait a bit to wait for monsters to enter view
                Wait(dur::Fixed(50)),
            ]);
            self.rotation_movement_tries += 1;

            // Transition to next state
            return State::SearchingForEnemy;
        }

        // Check whether bot should stay in area
        let circle_pattern_rotation_duration = config.farming_config().circle_pattern_rotation_duration();
        if circle_pattern_rotation_duration > 0 {
            self.move_circle_pattern(circle_pattern_rotation_duration);
        } else {
            self.rotation_movement_tries = 0;
            return self.state;
        }
        // Transition to next state
        State::SearchingForEnemy
    }

    fn move_circle_pattern(&self, rotation_duration: u64) {
        // low rotation duration means big circle, high means little circle
        use crate::movement::prelude::*;
        play!(self.movement => [
            HoldKeys(vec!["W", "Space", "D"]),
            Wait(dur::Fixed(rotation_duration)),
            ReleaseKey("D"),
            Wait(dur::Fixed(20)),
            ReleaseKeys(vec!["Space", "W"]),
            HoldKeyFor("S", dur::Fixed(50)),
        ]);
    }

    fn on_searching_for_enemy(
        &mut self,
        bot_config: &BotConfig,
        config: &FarmingConfig,
        image: &mut ImageAnalyzer,
    ) -> State {
        let mobs = image.identify_mobs(bot_config);
        if mobs.is_empty() {
            if config.is_manual_targetting() {
                // Transition to next state
                State::SearchingForEnemy
            } else {
                // Transition to next state
                State::NoEnemyFound
            }
        } else {
            // Calculate max distance of mobs
            let max_distance = match config.circle_pattern_rotation_duration() == 0 {
                true => 325,
                false => 1000,
            };

            // Get aggressive mobs to prioritize them
            let mut mob_list = mobs
                .iter()
                .filter(|m| m.target_type == TargetType::Mob(MobType::Aggressive))
                .cloned()
                .collect::<Vec<_>>();

            // Check if there's aggressive mobs otherwise collect passive mobs
            if mob_list.is_empty()
                || self.last_killed_type == MobType::Aggressive
                    && mob_list.len() == 1
                    && self.last_kill_time.elapsed().as_millis() < 5000
            {

                mob_list = mobs
                    .iter()
                    .filter(|m| m.target_type == TargetType::Mob(MobType::Passive))
                    .cloned()
                    .collect::<Vec<_>>();

            }

            // Check again
            if !mob_list.is_empty() {
                self.rotation_movement_tries = 0;
                //slog::debug!(self.logger, "Found mobs"; "mob_type" => mob_type, "mob_count" => mob_list.len());
                if let Some(mob) = {
                    // Try avoiding detection of last killed mob
                    if self.avoided_bounds.len() > 0 {
                        image.find_closest_mob(
                            mob_list.as_slice(),
                            Some(&self.avoided_bounds),
                            max_distance,
                            self.logger,
                        )
                    } else {
                        image.find_closest_mob(mob_list.as_slice(), None, max_distance, self.logger)
                    }
                } {
                    State::EnemyFound(*mob)
                } else {
                    // Transition to next state
                    State::SearchingForEnemy
                }
            } else {
                if config.is_manual_targetting() {
                    // Transition to next state
                    State::SearchingForEnemy
                } else {
                    // Transition to next state
                    State::NoEnemyFound
                }
            }
        }
    }

    fn avoid_last_click(&mut self) {
        if let Some(point) = self.last_click_pos {
            let mut marker = Bounds::default();
            marker.x = point.x - 1;
            marker.y = point.y - 1;
            marker.w = 2;
            marker.h = 2;
            self.avoided_bounds.push((marker, Instant::now(), 5000));
        }
    }

    fn on_enemy_found(&mut self, bot_config: &BotConfig, image: &ImageAnalyzer, mob: Target, frontend_info: &mut FrontendInfo) -> State {

        frontend_info.set_last_mob_bounds(mob.bounds.w, mob.bounds.h);
        if bot_config.whitelist_enabled() && !bot_config.farming_config().is_manual_targetting() {
            if !bot_config.match_whitelist(mob) {
                if mob.target_type == TargetType::Mob(MobType::Aggressive) {
                    self.key_manager.eval_avoid_mob_click(mob.get_active_avoid_coords(100));
                    return State::SearchingForEnemy;
                } else {
                    return State::SearchingForEnemy;
                }
            }
        }else if bot_config.farming_config().is_manual_targetting()  {
            if image.client_stats.target_hp.value > 0 {
                return State::Attacking(mob);
            } else {
                return State::SearchingForEnemy;
            }
        }
        self.last_no_ennemy_time = None;
        // Transform attack coords into local window coords
        let point = mob.get_attack_coords();

        self.last_click_pos = Some(point);

        // Set cursor position and simulate a click
        self.key_manager.eval_mob_click(point);

        // Wait a few ms before transitioning state
        std::thread::sleep(Duration::from_millis(500));
        State::Attacking(mob)
    }

    fn abort_attack(&mut self, image: &mut ImageAnalyzer) -> State {
        use crate::movement::prelude::*;
        self.is_attacking = false;

        if self.already_attack_count > 0 {
            // Target marker found
            if let Some(marker) = image.client_stats.target_marker {
                self.avoided_bounds.push((
                    marker.bounds.grow_by(self.already_attack_count * 10),
                    Instant::now(),
                    2000,
                ));
                self.already_attack_count += 1;
            }
        } else {
            self.obstacle_avoidance_count = 0;
            self.avoid_last_click();
        }
        play!(self.movement => [
            PressKey("Escape"),
        ]);
        return State::SearchingForEnemy;
    }

    fn avoid_obstacle(
        &mut self,
        image: &mut ImageAnalyzer,
        max_avoid: u32,
    ) -> bool {
        if self.obstacle_avoidance_count < max_avoid {
            use crate::movement::prelude::*;
            if self.obstacle_avoidance_count == 0 {
                play!(self.movement => [
                    PressKey("Z"),
                    HoldKeys(vec!["W", "Space"]),
                    Wait(dur::Fixed(800)),
                    ReleaseKeys(vec!["Space", "W"]),
                ]);
            } else {
                let rotation_key = ["A", "D"].choose(&mut self.rng).unwrap_or(&"A");
                // Move into a random direction while jumping
                play!(self.movement => [
                    HoldKeys(vec!["W", "Space"]),
                    HoldKeyFor(*rotation_key, dur::Fixed(200)),
                    Wait(dur::Fixed(800)),
                    ReleaseKeys(vec!["Space", "W"]),
                    PressKey("Z"),
                ]);
            }

            image.client_stats.target_hp.reset_last_update_time();
            self.obstacle_avoidance_count += 1;
            return false;
        } else {
            self.abort_attack(image);
            return true;
        }
    }

    fn on_attacking(
        &mut self,
        bot_config: &BotConfig,
        config: &FarmingConfig,
        mob: Target,
        image: &mut ImageAnalyzer,
    ) -> State {
        let is_npc =
            image.client_stats.target_hp.value == 100 && image.client_stats.target_mp.value == 0;
        let is_mob =
            image.client_stats.target_hp.value > 0 && image.client_stats.target_mp.value > 0;
        let is_mob_alive = image.client_stats.target_marker.is_some() || image.client_stats.target_mp.value > 0 || image.client_stats.target_hp.value > 0 ;

        if !self.is_attacking && !config.is_manual_targetting() {
            if is_npc {
                self.avoid_last_click();
                return State::SearchingForEnemy;
            } else if is_mob {
                self.rotation_movement_tries = 0;
                let hp_last_update = image.client_stats.hp.last_update_time.unwrap();

                // Detect if mob was attacked
                if image.client_stats.target_hp.value < 100 && config.prevent_already_attacked() {
                    // If we didn't took any damages abort attack
                    if hp_last_update.elapsed().as_millis() > 5000 {
                        return self.abort_attack(image);
                    } else {
                        if self.stealed_target_count > 5 {
                            self.stealed_target_count = 0;
                            self.already_attack_count = 1;
                        }
                    }
                }
            } else {
                // Not a mob we go search for another
                self.avoid_last_click();
                return State::SearchingForEnemy;
            }
        } else if !self.is_attacking && config.is_manual_targetting() {
            if !is_mob {
                return State::SearchingForEnemy;
            }
        }

        if is_mob_alive {
            // Engagin combat
            if !self.is_attacking {
                self.obstacle_avoidance_count = 0;
                self.last_initial_attack_time = Instant::now();
                self.is_attacking = true;
                self.already_attack_count = 0;
            }

            let last_target_hp_update = image
                .client_stats
                .target_hp
                .last_update_time
                .unwrap()
                .elapsed()
                .as_millis();

            // Obstacle avoidance
            if !config.is_manual_targetting() {
                if image.client_stats.target_marker.is_none() || last_target_hp_update > bot_config.obstacle_avoidance_cooldown() {
                    if image.client_stats.target_hp.value == 100 {
                        if self.avoid_obstacle(image, 2) {
                            return State::SearchingForEnemy;
                        }
                    }else {
                        if self.avoid_obstacle(image, config.obstacle_avoidance_max_try()) {
                            return State::SearchingForEnemy;
                        }
                    }
                } else {
                    self.obstacle_avoidance_count = 0;
                }
            }

            // Use buffs only when target is found so we don't waste them
            self.slots_usage.check_buffs();

            // Try to use attack skill if at least one is selected in slot bar
            self.slots_usage.get_slot_for(None, SlotType::AttackSkill, true);

            return self.state;
        } else if !is_mob_alive && self.is_attacking { // removed check is_alive
            // Mob's dead
            match mob.target_type {
                TargetType::Mob(MobType::Aggressive) => self.last_killed_type = MobType::Aggressive,
                TargetType::Mob(MobType::Passive) => self.last_killed_type = MobType::Passive,
                TargetType::TargetMarker => {}
            }

            self.is_attacking = false;
            return State::AfterEnemyKill(mob);
        } else {
            self.is_attacking = false;
            return State::SearchingForEnemy;
        }
    }

    fn after_enemy_kill_debug(&mut self, frontend_info: &mut FrontendInfo) {
        // Let's introduce some stats
        let started_elapsed = self.start_time.elapsed();
        let started_formatted = DateTime::format_time(started_elapsed);

        let elapsed_time_to_kill = self.last_initial_attack_time.elapsed();
        let elapsed_search_time = self.last_kill_time.elapsed() - elapsed_time_to_kill;

        let search_time_as_secs = {
            if self.kill_count > 0 {
                elapsed_search_time.as_secs_f32()
            } else {
                elapsed_search_time.as_secs_f32() - started_elapsed.as_secs_f32()
            }
        };
        let time_to_kill_as_secs = elapsed_time_to_kill.as_secs_f32();

        let kill_per_minute =
            DateTime::format_float(60.0 / (time_to_kill_as_secs + search_time_as_secs), 0);
        let kill_per_hour = DateTime::format_float(kill_per_minute * 60.0, 0);

        let elapsed_search_time_string = format!("{}secs", DateTime::format_float(search_time_as_secs, 2));
        let elapsed_time_to_kill_string =
            format!("{}secs", DateTime::format_float(time_to_kill_as_secs, 2));

        let elapsed = format!(
            "Elapsed time : since start {} to kill {} to find {} ",
            started_formatted, elapsed_time_to_kill_string, elapsed_search_time_string
        );
        slog::debug!(self.logger, "Monster was killed {}", elapsed);

        frontend_info.set_kill_stats((kill_per_minute, kill_per_hour), ( elapsed_search_time.as_millis(), elapsed_time_to_kill.as_millis() ))
    }

    fn after_enemy_kill(
        &mut self,
        frontend_info: &mut FrontendInfo,
    ) -> State {
        self.kill_count += 1;
        frontend_info.set_kill_count(self.kill_count);
        self.after_enemy_kill_debug(frontend_info);

        self.stealed_target_count = 0;
        self.last_kill_time = Instant::now();

        // Pickup items
        self.pickup_items();

        // Transition state
        State::SearchingForEnemy
    }
}
