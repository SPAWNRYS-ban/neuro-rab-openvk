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
        info!("🚀 Starting LongPoll event loop with auto-reconnection");
        info!("Config: max_reconnect_attempts={}, backoff_multiplier={}, max_wait_secs={}", 
            self.config.longpoll_max_reconnect_attempts,
            self.config.longpoll_backoff_multiplier,
            self.config.longpoll_max_wait_secs
        );
        
        loop {
            info!("📍 Entering connect_and_listen loop (reconnect_attempts: {})", self.reconnect_attempts);
            match self.connect_and_listen(&mut event_handler).await {
                Ok(_) => {
                    // Success - reset reconnection counters
                    info!("LongPoll connection closed normally, attempting to reconnect...");
                    self.reconnect_attempts = 0;
                    self.current_wait_interval = self.config.longpoll_reconnect_interval_secs;
                }
                Err(e) => {
                    error!("🔴 LongPoll connection error: {}. Attempting reconnection...", e);
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
        info!("🔗 Getting LongPoll server connection info...");
        let mut lp_server = self.openvk_client.messages_get_longpoll_server().await?;
        info!("✅ Connected to LongPoll server at: {} (key: {})", 
            lp_server.server, 
            &lp_server.key[..lp_server.key.len().min(10)]
        );

        // Listen in a loop
        let mut listen_cycle = 0;
        loop {
            listen_cycle += 1;
            // Only log first cycle and every 50th cycle to avoid spam
            if listen_cycle == 1 || listen_cycle % 50 == 0 {
                info!("🔄 LongPoll listen cycle #{}", listen_cycle);
            }
            
            match self
                .openvk_client
                .longpoll_listen(&mut lp_server)
                .await
            {
                Ok(notifications) => {
                    if !notifications.is_empty() {
                        info!("📬 Processing {} notifications from LongPoll", notifications.len());
                        
                        for (idx, notification) in notifications.iter().enumerate() {
                            info!(
                                "  [{}] Processing notification: event_type={:?}, peer_id={}, message_id={}", 
                                idx, notification.event_type, notification.peer_id, notification.message_id
                            );
                            
                            match event_handler(notification.clone()).await {
                                Ok(_) => {
                                    info!("  [{}] ✅ Successfully handled notification", idx);
                                }
                                Err(e) => {
                                    error!("  [{}] ❌ Error handling notification: {}", idx, e);
                                    // Continue processing other notifications
                                }
                            }
                        }
                    } else {
                        // No notifications this cycle - this is normal
                        // Don't log here to avoid spam, just continue
                    }
                }
                Err(e) => {
                    // Connection error - break and reconnect
                    error!("🔴 LongPoll connection lost during cycle #{}: {}", listen_cycle, e);
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
