use crate::config::Config;
use crate::openvk::{OpenVKClient, ParsedNotification};
use anyhow::{anyhow, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

/// Manages LongPoll connection with automatic reconnection and exponential backoff
pub struct LongPollManager {
    openvk_client: Arc<OpenVKClient>,
    config: Config,
    reconnect_attempts: u32,
    current_wait_interval: u64,
}

impl LongPollManager {
    pub fn new(openvk_client: Arc<OpenVKClient>, config: Config) -> Self {
        let current_wait_interval = config.longpoll_reconnect_interval_secs;
        Self {
            openvk_client,
            config,
            reconnect_attempts: 0,
            current_wait_interval,
        }
    }

    /// Main loop that handles LongPoll connection with automatic reconnection
    pub async fn run_with_reconnect<F, Fut>(&mut self, mut event_handler: F) -> Result<()>
    where
        F: FnMut(ParsedNotification) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<()>>,
    {
        loop {
            match self.connect_and_listen(&mut event_handler).await {
                Ok(_) => {
                    // Success - reset reconnection counters
                    info!("LongPoll connection closed normally, attempting to reconnect...");
                    self.reconnect_attempts = 0;
                    self.current_wait_interval = self.config.longpoll_reconnect_interval_secs;
                }
                Err(e) => {
                    error!("LongPoll connection error: {}. Attempting reconnection...", e);
                    self.handle_reconnect().await?;
                }
            }
        }
    }

    /// Establish connection and listen for events
    async fn connect_and_listen<F, Fut>(&mut self, event_handler: &mut F) -> Result<()>
    where
        F: FnMut(ParsedNotification) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<()>>,
    {
        // Get LongPoll server info
        let mut lp_server = self.openvk_client.messages_get_longpoll_server().await?;
        info!("Connected to LongPoll server");

        // Listen in a loop
        loop {
            match self
                .openvk_client
                .longpoll_listen(&mut lp_server)
                .await
            {
                Ok(notifications) => {
                    // Process each notification
                    for notification in notifications {
                        match event_handler(notification.clone()).await {
                            Ok(_) => {}
                            Err(e) => {
                                error!("Error handling notification: {}", e);
                                // Continue processing other notifications
                            }
                        }
                    }
                }
                Err(e) => {
                    // Connection error - break and reconnect
                    error!("LongPoll connection lost: {}", e);
                    return Err(e);
                }
            }
        }
    }

    /// Get reference to the OpenVK client
    pub fn get_client(&self) -> &Arc<OpenVKClient> {
        &self.openvk_client
    }

    /// Handle reconnection with exponential backoff
    async fn handle_reconnect(&mut self) -> Result<()> {
        self.reconnect_attempts += 1;

        if self.reconnect_attempts > self.config.longpoll_max_reconnect_attempts {
            return Err(anyhow!(
                "Max reconnection attempts ({}) exceeded",
                self.config.longpoll_max_reconnect_attempts
            ));
        }

        info!(
            "Reconnection attempt {}/{} in {} seconds",
            self.reconnect_attempts,
            self.config.longpoll_max_reconnect_attempts,
            self.current_wait_interval
        );

        // Wait before reconnecting
        sleep(Duration::from_secs(self.current_wait_interval)).await;

        // Calculate next interval with exponential backoff
        let next_interval = (self.current_wait_interval as f64
            * self.config.longpoll_backoff_multiplier) as u64;
        self.current_wait_interval = next_interval.min(self.config.longpoll_max_wait_secs);

        info!(
            "Next reconnection wait interval: {} seconds",
            self.current_wait_interval
        );

        Ok(())
    }

    /// Reset connection state (useful after successful reconnection)
    pub fn reset(&mut self) {
        self.reconnect_attempts = 0;
        self.current_wait_interval = self.config.longpoll_reconnect_interval_secs;
    }

    /// Get current reconnection attempt count
    pub fn get_reconnect_attempts(&self) -> u32 {
        self.reconnect_attempts
    }
}
