/// Image attachment handling module
/// Extracts images from OpenVK comment attachments, downloads them, and encodes to base64

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, warn};

/// Extract image URLs from OpenVK attachment array
/// OpenVK attachments format: [{ "type": "photo", "photo": { "sizes": [...] } }, ...]
pub fn extract_image_urls_from_attachments(attachments: &[Value]) -> Vec<String> {
    let mut urls = Vec::new();

    debug!("🔍 extract_image_urls_from_attachments called with {} attachments", attachments.len());

    for (idx, attachment) in attachments.iter().enumerate() {
        debug!("  Attachment #{}: {}", idx, serde_json::to_string_pretty(attachment).unwrap_or_default());
        
        let att_type = attachment
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("");

        debug!("    Type: '{}'", att_type);

        // Handle photo attachments
        if att_type == "photo" {
            if let Some(photo) = attachment.get("photo") {
                debug!("    Has 'photo' field");
                // Try to extract the largest image from sizes array
                if let Some(sizes) = photo.get("sizes").and_then(|s| s.as_array()) {
                    debug!("    Has 'sizes' array with {} items", sizes.len());
                    // Find the largest size (usually last or with highest width)
                    if let Some(largest) = sizes.last() {
                        if let Some(src) = largest.get("url").and_then(|s| s.as_str()) {
                            urls.push(src.to_string());
                            debug!("    ✅ Extracted photo URL: {}", truncate_url(src));
                        } else {
                            debug!("    ❌ No 'url' in largest size");
                        }
                    }
                } else {
                    debug!("    ❌ No 'sizes' array found");
                }
            } else {
                debug!("    ❌ No 'photo' field found");
            }
        } else {
            debug!("    ⏭️  Skipping non-photo type: '{}'", att_type);
        }
    }

    debug!("🔍 extract_image_urls_from_attachments: Found {} URLs total", urls.len());
    urls
}

/// Download image from URL and return raw bytes
pub async fn download_image(url: &str) -> Result<Vec<u8>> {
    debug!("Downloading image from: {}", truncate_url(url));

    let client = Client::new();
    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (compatible; NeuroBot/1.0)")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to download image: HTTP {}",
            response.status()
        ));
    }

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("image/jpeg");

    // Check if it's actually an image
    if !content_type.starts_with("image/") {
        return Err(anyhow!(
            "Downloaded content is not an image: {}",
            content_type
        ));
    }

    let bytes = response.bytes().await?;

    if bytes.is_empty() {
        return Err(anyhow!("Downloaded image is empty"));
    }

    Ok(bytes.to_vec())
}

/// Encode bytes to base64 string
pub fn encode_to_base64(data: &[u8]) -> String {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    engine.encode(data)
}

/// Guess MIME type from image bytes
/// Returns "image/jpeg", "image/png", "image/webp", or falls back to "image/jpeg"
pub fn guess_mime_type(data: &[u8]) -> String {
    if data.len() < 4 {
        return "image/jpeg".to_string();
    }

    // Check PNG signature
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return "image/png".to_string();
    }

    // Check JPEG signature
    if data.starts_with(b"\xff\xd8\xff") {
        return "image/jpeg".to_string();
    }

    // Check WebP signature
    if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
        return "image/webp".to_string();
    }

    // Check GIF signature
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return "image/gif".to_string();
    }

    // Default to JPEG
    "image/jpeg".to_string()
}

/// Process a single image from URL: download and encode to base64 with MIME type
/// Optionally validates size limit (in MB) for Vision API compatibility
pub async fn process_image(url: &str) -> Result<(String, String)> {
    process_image_with_size_limit(url, None).await
}

/// Process image with optional size limit validation
/// size_limit_mb: if Some(n), rejects images larger than n MB (for API limits)
pub async fn process_image_with_size_limit(
    url: &str,
    size_limit_mb: Option<u64>,
) -> Result<(String, String)> {
    match download_image(url).await {
        Ok(data) => {
            let size_mb = (data.len() as f64) / (1024.0 * 1024.0);
            
            // Check size limit if provided
            if let Some(limit_mb) = size_limit_mb {
                if (data.len() as u64) > limit_mb * 1024 * 1024 {
                    return Err(anyhow!(
                        "Image too large: {:.2} MB exceeds limit of {} MB",
                        size_mb,
                        limit_mb
                    ));
                }
            }
            
            let mime_type = guess_mime_type(&data);
            let base64 = encode_to_base64(&data);
            debug!(
                "Successfully processed image from {}: {:.2} MB ({} bytes), type: {}",
                truncate_url(url),
                size_mb,
                data.len(),
                mime_type
            );
            Ok((base64, mime_type))
        }
        Err(e) => {
            warn!("Failed to process image from {}: {}", truncate_url(url), e);
            Err(e)
        }
    }
}

/// Truncate URL for logging (show only domain and short path)
fn truncate_url(url: &str) -> String {
    if url.len() > 80 {
        format!("{}...", &url[..77])
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_type_detection() {
        // PNG signature
        let png_data = b"\x89PNG\r\n\x1a\nrest";
        assert_eq!(guess_mime_type(png_data), "image/png");

        // JPEG signature
        let jpeg_data = b"\xff\xd8\xffrest";
        assert_eq!(guess_mime_type(jpeg_data), "image/jpeg");

        // WebP signature
        let webp_data = b"RIFFxxxxWEBPrest";
        assert_eq!(guess_mime_type(webp_data), "image/webp");

        // Unknown falls back to JPEG
        let unknown = b"unknownformat";
        assert_eq!(guess_mime_type(unknown), "image/jpeg");
    }

    #[test]
    fn test_base64_encoding() {
        let data = b"hello world";
        let encoded = encode_to_base64(data);
        assert_eq!(encoded, "aGVsbG8gd29ybGQ=");
    }
}
