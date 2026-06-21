use anyhow::Result;

use crate::{broker, config::Config, db::Database, scheduler, worker};

#[derive(Clone, Debug)]
pub struct DaemonOptions {
    pub once: bool,
    pub concurrency: usize,
    pub scheduler_interval_seconds: u64,
    pub idle_interval_seconds: u64,
}

pub async fn run(config: Config, db: Database, options: DaemonOptions) -> Result<()> {
    let scheduler_interval =
        std::time::Duration::from_secs(options.scheduler_interval_seconds.max(1));
    let idle_interval = std::time::Duration::from_secs(options.idle_interval_seconds.max(1));
    let concurrency = options.concurrency.max(1);

    println!(
        "Librarian daemon started. concurrency={concurrency}, scheduler_interval={}s, idle_interval={}s",
        scheduler_interval.as_secs(),
        idle_interval.as_secs()
    );

    let broker_bind = config.broker.bind.clone();
    let broker_db = db.clone();
    let broker_config = config.clone();
    tokio::spawn(async move {
        if let Err(error) = broker::serve(broker_bind, broker_db, broker_config).await {
            eprintln!("Librarian broker stopped: {error}");
        }
    });

    if options.once {
        run_once(config, db, concurrency).await?;
        return Ok(());
    }

    let mut last_scheduler_tick = std::time::Instant::now() - scheduler_interval;
    loop {
        if last_scheduler_tick.elapsed() >= scheduler_interval {
            let report = scheduler::tick(&db, &config).await?;
            if report.ran_schedules > 0 || report.heartbeat_missed > 0 {
                println!(
                    "Scheduler tick: ran_schedules={}, heartbeat_missed={}",
                    report.ran_schedules, report.heartbeat_missed
                );
            }
            last_scheduler_tick = std::time::Instant::now();
        }

        let ran = worker::run_batch(config.clone(), db.clone(), concurrency).await?;
        if ran > 0 {
            println!("Worker ran {ran} job(s).");
        } else {
            tokio::time::sleep(idle_interval).await;
        }
    }
}

async fn run_once(config: Config, db: Database, concurrency: usize) -> Result<()> {
    let report = scheduler::tick(&db, &config).await?;
    println!(
        "Scheduler tick: ran_schedules={}, heartbeat_missed={}",
        report.ran_schedules, report.heartbeat_missed
    );
    let ran = worker::run_batch(config, db, concurrency).await?;
    if ran == 0 {
        println!("No queued jobs.");
    } else {
        println!("Worker ran {ran} job(s).");
    }
    Ok(())
}
