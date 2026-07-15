use crate::{
    cleanup::{history_from_result, perform_cleanup},
    models::{
        CleanRequest, GrowthAlert, ScheduleFrequency, SchedulerSettings, SizeSample, VolumeStatus,
    },
    platform::volume_status,
    process::{claude_activity, claude_activity_blocks_cleanup},
    quarantine::clear_expired_quarantine,
    safety::claude_roots,
    scanner::perform_scan,
    state::{AppState, RootSignature, SchedulerScanCache},
};
use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use std::{
    cmp::Reverse,
    fs,
    sync::{atomic::Ordering, Arc},
    thread,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

const SCHEDULER_TICK_SECONDS: u64 = 30;
const SCHEDULER_SCAN_INTERVAL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
struct DueTrigger {
    trigger: String,
    occurrence_key: Option<String>,
}

pub fn start_scheduler(app: tauri::AppHandle) {
    thread::spawn(move || {
        let launched_at = Instant::now();
        let mut last_quarantine_sweep = Instant::now() - Duration::from_secs(60 * 60);
        loop {
            thread::sleep(Duration::from_secs(SCHEDULER_TICK_SECONDS));
            let Some(state_ref) = app.try_state::<Arc<AppState>>() else {
                continue;
            };
            let state = state_ref.inner().clone();
            if last_quarantine_sweep.elapsed() >= Duration::from_secs(60 * 60) {
                let _ = clear_expired_quarantine(&state.quarantine_root);
                last_quarantine_sweep = Instant::now();
            }
            let due = match determine_trigger(&state, launched_at.elapsed()) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let Some(due) = due else {
                continue;
            };
            let settings = match state.settings() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !automation_activity_allowed(claude_activity(), settings.clean_when_claude_running) {
                continue;
            }
            if due.trigger == "startup"
                && state
                    .startup_evaluated
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
            {
                continue;
            }
            let candidates = match safe_cleanup_candidates(&settings, &due.trigger) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if candidates.is_empty() {
                if let Some(key) = due.occurrence_key {
                    let _ = mark_schedule_occurrence(&state, key);
                }
                continue;
            }
            if let Some(key) = due.occurrence_key {
                if mark_schedule_occurrence(&state, key).is_err() {
                    continue;
                }
            }
            let request = CleanRequest {
                paths: candidates,
                allow_when_running: settings.clean_when_claude_running,
                quarantine_caution: false,
                trigger: due.trigger,
            };
            if let Ok(result) = perform_cleanup(
                &request,
                &state.quarantine_root,
                settings.quarantine_retention_days,
            ) {
                let _ = state.push_history(history_from_result(&result));
                if settings.notification_behavior != "silent" {
                    let _ = app.emit("cleanup-completed", result);
                }
            }
        }
    });
}

fn determine_trigger(
    state: &Arc<AppState>,
    uptime: Duration,
) -> Result<Option<DueTrigger>, String> {
    let settings = state.settings()?;
    if !settings.enabled {
        return Ok(None);
    }
    let now = Local::now();
    if settings.schedule_enabled && settings.schedule_frequency != ScheduleFrequency::Startup {
        if let Some((key, occurrence)) = schedule_occurrence(now.naive_local(), &settings) {
            let age = now.naive_local() - occurrence;
            let already_ran = state
                .persisted
                .lock()
                .map_err(|_| "State lock failed".to_string())?
                .last_schedule_occurrence
                .as_deref()
                == Some(&key);
            if !already_ran
                && age >= ChronoDuration::zero()
                && age <= ChronoDuration::minutes(i64::from(settings.schedule_grace_minutes))
            {
                return Ok(Some(DueTrigger {
                    trigger: "schedule".to_string(),
                    occurrence_key: Some(key),
                }));
            }
        }
    }
    let startup_requested = settings.startup_cleanup_enabled
        || (settings.schedule_enabled && settings.schedule_frequency == ScheduleFrequency::Startup);
    let in_cooldown = cooldown_active(state, &settings)?;
    if startup_requested
        && !in_cooldown
        && uptime >= Duration::from_secs(u64::from(settings.startup_cleanup_delay_seconds))
        && !state.startup_evaluated.load(Ordering::SeqCst)
    {
        return Ok(Some(DueTrigger {
            trigger: "startup".to_string(),
            occurrence_key: None,
        }));
    }
    if in_cooldown {
        return Ok(None);
    }
    if settings.disk_space_enabled {
        let status = volume_status(&settings.monitored_volume)?;
        if is_below_free_threshold(&status, &settings) {
            return Ok(Some(DueTrigger {
                trigger: "disk_space".to_string(),
                occurrence_key: None,
            }));
        }
    }
    if settings.threshold_enabled {
        let Some(signature) = next_scheduler_scan_signature(state)? else {
            return Ok(None);
        };
        let scan = perform_scan(state.warnings())?;
        let _ = state.record_sample(scan.total_bytes);
        mark_scheduler_scan_complete(state, signature)?;
        if bytes_to_gb(scan.total_bytes) >= settings.threshold_gb {
            return Ok(Some(DueTrigger {
                trigger: "threshold".to_string(),
                occurrence_key: None,
            }));
        }
    }
    Ok(None)
}

fn safe_cleanup_candidates(
    settings: &SchedulerSettings,
    trigger: &str,
) -> Result<Vec<String>, String> {
    let scan = perform_scan(Vec::new())?;
    let mut candidates = Vec::new();
    for root in &scan.roots {
        collect_default_paths(root, &mut candidates);
    }
    if trigger != "disk_space" {
        return Ok(candidates.into_iter().map(|(path, _)| path).collect());
    }
    let monitored = volume_status(&settings.monitored_volume)?;
    candidates.retain(|(path, _)| {
        volume_status(path)
            .map(|status| same_volume_identifier(&status.volume, &monitored.volume))
            .unwrap_or(false)
    });
    candidates.sort_by_key(|item| Reverse(item.1));
    let target_bytes = gb_to_bytes(settings.target_free_gb);
    let bytes_needed = target_bytes.saturating_sub(monitored.available_bytes);
    if bytes_needed == 0 {
        return Ok(Vec::new());
    }
    Ok(select_candidates_with_limit(
        &candidates,
        settings.max_cleanup_bytes.min(bytes_needed),
    ))
}

fn collect_default_paths(node: &crate::models::CacheNode, paths: &mut Vec<(String, u64)>) {
    if node.exists && node.default_cleanup && node.size_bytes > 0 {
        paths.push((node.path.clone(), node.size_bytes));
        return;
    }
    for child in &node.children {
        collect_default_paths(child, paths);
    }
}

fn select_candidates_with_limit(candidates: &[(String, u64)], max_bytes: u64) -> Vec<String> {
    let mut selected = Vec::new();
    let mut total = 0_u64;
    for (path, bytes) in candidates {
        if *bytes == 0 || *bytes > max_bytes.saturating_sub(total) {
            continue;
        }
        selected.push(path.clone());
        total = total.saturating_add(*bytes);
        if total >= max_bytes {
            break;
        }
    }
    selected
}

fn schedule_occurrence(
    now: NaiveDateTime,
    settings: &SchedulerSettings,
) -> Option<(String, NaiveDateTime)> {
    let time = NaiveTime::parse_from_str(&settings.schedule_time, "%H:%M").ok()?;
    let occurrence = match settings.schedule_frequency {
        ScheduleFrequency::Daily => {
            let today = now.date().and_time(time);
            if today <= now {
                today
            } else {
                (now.date() - ChronoDuration::days(1)).and_time(time)
            }
        }
        ScheduleFrequency::Weekly => {
            let wanted = settings.weekly_day.clamp(1, 7);
            let current = now.weekday().number_from_monday();
            let mut days_back = (7 + current - wanted) % 7;
            let mut candidate =
                (now.date() - ChronoDuration::days(i64::from(days_back))).and_time(time);
            if candidate > now {
                days_back += 7;
                candidate =
                    (now.date() - ChronoDuration::days(i64::from(days_back))).and_time(time);
            }
            candidate
        }
        ScheduleFrequency::Monthly => {
            monthly_occurrence(now, settings.monthly_day.clamp(1, 31), time)?
        }
        ScheduleFrequency::Startup => return None,
    };
    let key = format!(
        "{:?}:{}",
        settings.schedule_frequency,
        occurrence.format("%Y-%m-%dT%H:%M")
    );
    Some((key, occurrence))
}

fn monthly_occurrence(
    now: NaiveDateTime,
    wanted_day: u32,
    time: NaiveTime,
) -> Option<NaiveDateTime> {
    let current_day = wanted_day.min(last_day_of_month(now.year(), now.month())?);
    let current = NaiveDate::from_ymd_opt(now.year(), now.month(), current_day)?.and_time(time);
    if current <= now {
        return Some(current);
    }
    let (year, month) = if now.month() == 1 {
        (now.year() - 1, 12)
    } else {
        (now.year(), now.month() - 1)
    };
    let day = wanted_day.min(last_day_of_month(year, month)?);
    Some(NaiveDate::from_ymd_opt(year, month, day)?.and_time(time))
}

fn last_day_of_month(year: i32, month: u32) -> Option<u32> {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1)?;
    Some((first_next - ChronoDuration::days(1)).day())
}

fn is_below_free_threshold(status: &VolumeStatus, settings: &SchedulerSettings) -> bool {
    let below_bytes = bytes_to_gb(status.available_bytes) < settings.minimum_free_gb;
    let below_percent = settings
        .minimum_free_percent
        .map(|minimum| status.free_percentage < minimum)
        .unwrap_or(false);
    below_bytes || below_percent
}

fn cooldown_active(state: &Arc<AppState>, settings: &SchedulerSettings) -> Result<bool, String> {
    let last = state
        .persisted
        .lock()
        .map_err(|_| "State lock failed".to_string())?
        .last_cleanup_at;
    Ok(last
        .map(|value| {
            Local::now() - value < ChronoDuration::hours(i64::from(settings.cleanup_cooldown_hours))
        })
        .unwrap_or(false))
}

fn mark_schedule_occurrence(state: &Arc<AppState>, key: String) -> Result<(), String> {
    {
        let mut persisted = state
            .persisted
            .lock()
            .map_err(|_| "State lock failed".to_string())?;
        persisted.last_schedule_occurrence = Some(key);
    }
    state.save()
}

fn root_modification_signature() -> Vec<RootSignature> {
    claude_roots()
        .into_iter()
        .map(|root| RootSignature {
            modified: fs::metadata(&root.path)
                .and_then(|metadata| metadata.modified())
                .ok(),
            path: root.path,
        })
        .collect()
}

fn next_scheduler_scan_signature(
    state: &Arc<AppState>,
) -> Result<Option<Vec<RootSignature>>, String> {
    let signature = root_modification_signature();
    let cache = state
        .scheduler_scan_cache
        .lock()
        .map_err(|_| "Scheduler cache lock failed".to_string())?;
    let Some(cache) = cache.as_ref() else {
        return Ok(Some(signature));
    };
    let elapsed = (Local::now() - cache.scanned_at)
        .to_std()
        .unwrap_or_default();
    if elapsed < SCHEDULER_SCAN_INTERVAL && cache.root_signature == signature {
        Ok(None)
    } else {
        Ok(Some(signature))
    }
}

fn mark_scheduler_scan_complete(
    state: &Arc<AppState>,
    signature: Vec<RootSignature>,
) -> Result<(), String> {
    let mut cache = state
        .scheduler_scan_cache
        .lock()
        .map_err(|_| "Scheduler cache lock failed".to_string())?;
    *cache = Some(SchedulerScanCache {
        scanned_at: Local::now(),
        root_signature: signature,
    });
    Ok(())
}

pub fn normalize_settings(mut settings: SchedulerSettings) -> SchedulerSettings {
    if NaiveTime::parse_from_str(&settings.schedule_time, "%H:%M").is_err() {
        settings.schedule_time = "02:00".to_string();
    }
    settings.weekly_day = settings.weekly_day.clamp(1, 7);
    settings.monthly_day = settings.monthly_day.clamp(1, 31);
    settings.schedule_grace_minutes = settings.schedule_grace_minutes.clamp(1, 180);
    settings.threshold_gb = settings.threshold_gb.max(1.0);
    settings.minimum_free_gb = settings.minimum_free_gb.max(0.5);
    settings.minimum_free_percent = settings
        .minimum_free_percent
        .map(|value| value.clamp(1.0, 99.0));
    settings.target_free_gb = settings.target_free_gb.max(settings.minimum_free_gb);
    settings.cleanup_cooldown_hours = settings.cleanup_cooldown_hours.clamp(1, 168);
    settings.max_cleanup_bytes = settings
        .max_cleanup_bytes
        .clamp(64 * 1024 * 1024, 100 * 1024 * 1024 * 1024);
    if settings.notification_behavior != "silent" {
        settings.notification_behavior = "in_app".to_string();
    }
    settings.growth_alert_gb_per_hour = settings.growth_alert_gb_per_hour.max(0.1);
    settings.startup_cleanup_delay_seconds = settings.startup_cleanup_delay_seconds.clamp(5, 3600);
    if settings.launch_at_login {
        settings.start_minimized = true;
    }
    settings.quarantine_retention_days = match settings.quarantine_retention_days {
        -1 => -1,
        1 => 1,
        14 => 14,
        30 => 30,
        _ => 7,
    };
    settings
}

pub fn calculate_growth_alert(state: &crate::models::PersistedState) -> GrowthAlert {
    let samples = state.samples.iter().collect::<Vec<_>>();
    if samples.len() < 2 {
        return GrowthAlert {
            active: false,
            gb_per_hour: 0.0,
            baseline_gb_per_hour: 0.0,
            message: "Not enough samples to calculate growth rate.".to_string(),
        };
    }
    let latest = samples[samples.len() - 1];
    let previous = samples[samples.len() - 2];
    let gb_per_hour = rate_between(previous, latest);
    let baseline_gb_per_hour = if samples.len() >= 4 {
        let rates = samples
            .windows(2)
            .take(samples.len().saturating_sub(2))
            .map(|pair| rate_between(pair[0], pair[1]).max(0.0))
            .collect::<Vec<_>>();
        if rates.is_empty() {
            0.0
        } else {
            rates.iter().sum::<f64>() / rates.len() as f64
        }
    } else {
        0.0
    };
    let threshold = state.settings.growth_alert_gb_per_hour;
    let active = state.settings.growth_alert_enabled
        && gb_per_hour > threshold
        && gb_per_hour > baseline_gb_per_hour.max(0.1) * 2.0;
    GrowthAlert {
        active,
        gb_per_hour,
        baseline_gb_per_hour,
        message: if active {
            format!("Claude cache is growing at {:.1} GB/hour, above the {:.1} GB/hour alert threshold.", gb_per_hour, threshold)
        } else {
            "Growth rate is within the learned baseline.".to_string()
        },
    }
}

fn rate_between(previous: &SizeSample, latest: &SizeSample) -> f64 {
    let seconds = (latest.captured_at - previous.captured_at)
        .num_seconds()
        .max(1) as f64;
    let delta = latest.total_bytes.saturating_sub(previous.total_bytes);
    bytes_to_gb(delta) / (seconds / 3600.0)
}

fn bytes_to_gb(bytes: u64) -> f64 {
    bytes as f64 / 1024_f64.powi(3)
}

fn gb_to_bytes(gb: f64) -> u64 {
    (gb.max(0.0) * 1024_f64.powi(3)).min(u64::MAX as f64) as u64
}

fn same_volume_identifier(left: &str, right: &str) -> bool {
    if cfg!(target_os = "windows") {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

fn automation_activity_allowed(
    activity: crate::models::ClaudeActivity,
    allow_when_running: bool,
) -> bool {
    !claude_activity_blocks_cleanup(activity) || allow_when_running
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(frequency: ScheduleFrequency) -> SchedulerSettings {
        SchedulerSettings {
            schedule_frequency: frequency,
            schedule_time: "02:00".to_string(),
            schedule_grace_minutes: 30,
            ..Default::default()
        }
    }

    #[test]
    fn daily_schedule_has_grace_instead_of_exact_minute() {
        let now = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(2, 20, 0)
            .unwrap();
        let (_, occurrence) =
            schedule_occurrence(now, &settings(ScheduleFrequency::Daily)).unwrap();
        assert_eq!(now - occurrence, ChronoDuration::minutes(20));
    }

    #[test]
    fn weekly_schedule_uses_most_recent_occurrence() {
        let now = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(3, 0, 0)
            .unwrap(); // Wednesday
        let mut value = settings(ScheduleFrequency::Weekly);
        value.weekly_day = 3;
        let (_, occurrence) = schedule_occurrence(now, &value).unwrap();
        assert_eq!(occurrence.date(), now.date());
    }

    #[test]
    fn monthly_day_uses_last_valid_day() {
        let now = NaiveDate::from_ymd_opt(2026, 2, 28)
            .unwrap()
            .and_hms_opt(3, 0, 0)
            .unwrap();
        let mut value = settings(ScheduleFrequency::Monthly);
        value.monthly_day = 31;
        let (_, occurrence) = schedule_occurrence(now, &value).unwrap();
        assert_eq!(
            occurrence.date(),
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );
    }

    #[test]
    fn disk_threshold_can_use_bytes_or_percentage() {
        let status = VolumeStatus {
            volume: "C:\\".to_string(),
            available_bytes: 9 * 1024_u64.pow(3),
            total_bytes: 100 * 1024_u64.pow(3),
            free_percentage: 9.0,
        };
        let value = SchedulerSettings {
            minimum_free_gb: 10.0,
            minimum_free_percent: Some(8.0),
            ..Default::default()
        };
        assert!(is_below_free_threshold(&status, &value));
        let satisfied = VolumeStatus {
            available_bytes: 20 * 1024_u64.pow(3),
            free_percentage: 20.0,
            ..status
        };
        assert!(!is_below_free_threshold(&satisfied, &value));
    }

    #[test]
    fn cleanup_limit_skips_oversized_target() {
        let candidates = vec![
            ("a".to_string(), 8),
            ("b".to_string(), 5),
            ("c".to_string(), 3),
        ];
        assert_eq!(
            select_candidates_with_limit(&candidates, 7),
            vec!["b".to_string()]
        );
    }

    #[test]
    fn repeated_schedule_calculation_has_the_same_occurrence_key() {
        let now = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(2, 10, 0)
            .unwrap();
        let first = schedule_occurrence(now, &settings(ScheduleFrequency::Daily))
            .unwrap()
            .0;
        let second = schedule_occurrence(now, &settings(ScheduleFrequency::Daily))
            .unwrap()
            .0;
        assert_eq!(first, second);
    }

    #[test]
    fn cooldown_prevents_repeated_cleanup_loops() {
        let root = std::env::temp_dir().join(format!("ccw-cooldown-{}", std::process::id()));
        let state = Arc::new(AppState::for_test(
            root.join("state.json"),
            root.join("quarantine"),
        ));
        state.persisted.lock().unwrap().last_cleanup_at = Some(Local::now());
        let value = SchedulerSettings {
            cleanup_cooldown_hours: 6,
            ..Default::default()
        };
        assert!(cooldown_active(&state, &value).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn claude_activity_blocks_automation_by_default() {
        assert!(!automation_activity_allowed(
            crate::models::ClaudeActivity::Background,
            false
        ));
        assert!(!automation_activity_allowed(
            crate::models::ClaudeActivity::Window,
            false
        ));
        assert!(automation_activity_allowed(
            crate::models::ClaudeActivity::Window,
            true
        ));
    }

    #[test]
    fn volume_comparison_is_platform_aware() {
        if cfg!(target_os = "windows") {
            assert!(same_volume_identifier("C:\\", "c:\\"));
            assert!(!same_volume_identifier("C:\\", "D:\\"));
        }
    }

    #[test]
    fn no_safe_candidates_and_insufficient_candidates_stop_cleanly() {
        assert!(select_candidates_with_limit(&[], 10).is_empty());
        let insufficient = vec![("only".to_string(), 3)];
        assert_eq!(
            select_candidates_with_limit(&insufficient, 10),
            vec!["only".to_string()]
        );
    }

    #[test]
    fn notification_behavior_is_limited_to_supported_local_modes() {
        let mut unsupported = SchedulerSettings {
            notification_behavior: "remote".to_string(),
            ..SchedulerSettings::default()
        };
        unsupported = normalize_settings(unsupported);
        assert_eq!(unsupported.notification_behavior, "in_app");

        let silent = normalize_settings(SchedulerSettings {
            notification_behavior: "silent".to_string(),
            ..SchedulerSettings::default()
        });
        assert_eq!(silent.notification_behavior, "silent");
    }
}
