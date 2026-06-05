# Vision API Image Handling Fix

## Problem Statement
When wall posts contain images, the Claude API (via tokenator.cloud) was returning 400 "Bad Request" errors when attempting to process images via the Vision API. This issue was discovered during automatic wall post response functionality implementation.

## Root Cause Analysis
The problem was likely caused by:
1. **API Incompatibility**: tokenator.cloud may not fully support Rich Content Format with base64-encoded images
2. **Image Size**: Large base64-encoded images may exceed API size limits
3. **Missing Configuration**: No size validation or fallback mechanism for image processing failures

## Solution Overview

### 1. Configuration-Based Vision API Control
Added two new environment variables to allow fine-grained control:

```env
# Enable or disable Vision API entirely
ENABLE_VISION_API=true

# Maximum image size in MB for Vision requests
# Default: 20 MB (adjust down if API enforces stricter limits)
VISION_API_MAX_IMAGE_SIZE_MB=20
```

**Benefits:**
- Disabled entirely if tokenator.cloud doesn't support Rich Content Format
- Reduce from 20 MB to smaller values if API rejects specific sizes
- Can be toggled without recompilation

### 2. Image Size Validation Before Processing

Added `process_image_with_size_limit()` function in `src/image_handler.rs`:

```rust
/// Process image with optional size limit validation
/// size_limit_mb: if Some(n), rejects images larger than n MB (for API limits)
pub async fn process_image_with_size_limit(
    url: &str,
    size_limit_mb: Option<u64>,
) -> Result<(String, String)>
```

**Benefits:**
- Validates image size BEFORE attempting API call
- Returns clear error messages (e.g., "Image too large: 25.5 MB exceeds limit of 20 MB")
- Prevents wasted API quota on oversized images

### 3. Enhanced Error Logging

Updated `src/ai/claude.rs` with detailed debugging:

```rust
debug!("Adding image block: type={}, size={} bytes (base64)", mime_type, base64_data.len());
debug!("Sending vision request with {} image(s), total base64 size: {} MB", blocks.len() - 1, total_size);
```

Logs now show:
- Individual image MIME type and size
- Total number of images
- Total base64 payload size in MB
- Explicit success: "✅ Vision analysis succeeded"
- Explicit failure with error: "❌ Vision analysis failed (...), will fallback to text-only"

### 4. Graceful Fallback

The existing fallback mechanism in `src/main.rs` now handles vision errors properly:

```rust
match claude_ai.analyze_image_with_text(image_prompt, images_to_analyze).await {
    Ok(response) => {
        info!("✅ Vision analysis completed successfully");
        response
    }
    Err(e) => {
        error!("Failed to analyze image: {}", e);
        warn!("Falling back to text-only analysis due to vision error");
        claude_ai.generate_response_with_context(clean_text.clone(), context).await?
    }
}
```

**Fallback Flow:**
1. Try vision analysis with images
2. If vision fails → fall back to text-only response
3. Never blocks bot from responding

## Configuration Examples

### Option A: Strict Size Limits (Safe)
```env
ENABLE_VISION_API=true
VISION_API_MAX_IMAGE_SIZE_MB=5
```
- Only processes small images (thumbnails, screenshots)
- Lowest risk of API errors

### Option B: Moderate Size Limits (Balanced)
```env
ENABLE_VISION_API=true
VISION_API_MAX_IMAGE_SIZE_MB=10
```
- Processes typical web images
- Good balance between coverage and API stability

### Option C: High Size Limits (If API Supports)
```env
ENABLE_VISION_API=true
VISION_API_MAX_IMAGE_SIZE_MB=50
```
- Processes high-quality images
- Only if tokenator.cloud confirms support

### Option D: Disable Vision API Entirely
```env
ENABLE_VISION_API=false
```
- Falls back to text-only analysis for all posts
- Zero risk of 400 errors
- Fallback system handles gracefully

## Implementation Details

### Files Modified

#### src/config.rs
- Added `enable_vision_api: bool` field
- Added `vision_api_max_image_size_mb: u64` field
- Both default to true and 20 MB respectively

#### src/image_handler.rs
- Added `process_image_with_size_limit()` public function
- Original `process_image()` remains as wrapper calling the new function with `None` (no limit)
- Size validation occurs AFTER download but BEFORE base64 encoding
- Detailed logging of image size in both bytes and MB

#### src/main.rs
- Updated `generate_bot_response()` to check `config.enable_vision_api`
- Passes `Some(config.vision_api_max_image_size_mb)` to `process_image_with_size_limit()`
- Logs when Vision API is disabled: "Vision API is disabled in config"
- Enhanced warning on image processing failure: "Failed to process image from comment: {} (will skip this image)"
- Fallback already existed, now more robust with explicit error handling

#### src/ai/claude.rs
- Enhanced logging with image count and size info
- Added explicit success/failure logging
- Returns `Result` type for error handling

#### .env.example
- Added `ENABLE_VISION_API` with default `true`
- Added `VISION_API_MAX_IMAGE_SIZE_MB` with default `20`
- Added configuration documentation

## Testing Recommendations

### Test Case 1: Small Image Posting
**Setup:** Post with image < 5 MB
**Expected:** Vision analysis succeeds, detailed response about image

### Test Case 2: Large Image Posting
**Setup:** Post with image > configured limit
**Expected:** Image skipped, fallback to text-only response, logs show "Image too large: X MB exceeds limit"

### Test Case 3: Vision API Disabled
**Setup:** `ENABLE_VISION_API=false`, post images
**Expected:** Images skipped entirely, text-only response, logs show "Vision API is disabled in config"

### Test Case 4: Vision API Error
**Setup:** Force API error (invalid response), post with image
**Expected:** Graceful fallback to text-only, logs show error and fallback message

### Test Case 5: Multiple Images
**Setup:** Post with 3 small images
**Expected:** All 3 processed, vision analysis includes all, logs show individual image sizes

## Troubleshooting

**Issue:** "Image too large: X.XX MB exceeds limit of Y MB"
- **Solution:** Either increase `VISION_API_MAX_IMAGE_SIZE_MB` or improve image compression

**Issue:** "Vision analysis failed (Request failed), will fallback to text-only"
- **Solution 1:** Try reducing `VISION_API_MAX_IMAGE_SIZE_MB` (API may have strict limits)
- **Solution 2:** Try disabling with `ENABLE_VISION_API=false` (tokenator.cloud may not support Rich Content)

**Issue:** "Failed to download image: HTTP 404"
- **Solution:** Image URL is broken, bot handles gracefully and skips image

## Performance Impact
- Image size validation: O(1) operation after download
- Base64 encoding size calculation: Already happens during encoding
- Network impact: No additional requests
- Logging overhead: debug! logs only if debug logging enabled

## Backward Compatibility
- Default settings (`ENABLE_VISION_API=true`, `VISION_API_MAX_IMAGE_SIZE_MB=20`) attempt to maintain existing behavior
- If Vision API worked before the fixes, it will continue to work
- If it didn't work, the new configuration options allow quick fixes without code changes

## Future Improvements
1. **Image Compression**: Pre-compress images before base64 encoding
2. **Adaptive Size Limits**: Auto-detect API limits via test request
3. **Format Detection**: Only process supported formats (JPEG, PNG, WebP)
4. **Cached Analysis**: Store image analysis results for duplicate images
5. **Batch Processing**: Process multiple images more efficiently
