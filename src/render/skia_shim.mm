#include <cstdint>
#include <cstring>

#include "include/core/SkCanvas.h"
#include "include/core/SkData.h"
#include "include/core/SkImage.h"
#include "include/core/SkImageInfo.h"
#include "include/core/SkMaskFilter.h"
#include "include/core/SkMatrix.h"
#include "include/core/SkPaint.h"
#include "include/core/SkPath.h"
#include "include/core/SkPicture.h"
#include "include/core/SkPictureRecorder.h"
#include "include/core/SkRefCnt.h"
#include "include/core/SkSamplingOptions.h"
#include "include/core/SkShader.h"
#include "include/core/SkSurface.h"
#include "include/effects/SkImageFilters.h"
#include "include/encode/SkPngEncoder.h"

// Metal support
#ifdef __APPLE__
#include "include/core/SkFont.h"
#include "include/gpu/GrBackendSurface.h"
#include "include/gpu/GrDirectContext.h"
#include "include/gpu/mtl/GrMtlBackendContext.h"
#include "include/gpu/mtl/GrMtlTypes.h"
#include "include/gpu/ganesh/SkSurfaceGanesh.h"
#include "include/gpu/ganesh/mtl/SkSurfaceMetal.h"
#include <Metal/Metal.h>
#include <QuartzCore/CAMetalLayer.h>
#include <CoreFoundation/CoreFoundation.h>
#endif

extern "C" {
typedef uint32_t sk_color_t;

enum sk_colortype_t {
    sk_colortype_unknown = 0,
    sk_colortype_rgba8888 = 1,
    sk_colortype_bgra8888 = 2,
    sk_colortype_alpha8 = 3,
};

enum sk_alphatype_t {
    sk_alphatype_opaque_alpha = 0,
    sk_alphatype_premul_alpha = 1,
    sk_alphatype_unpremul_alpha = 2,
};

struct sk_imageinfo_t {
    int32_t width;
    int32_t height;
    sk_colortype_t colorType;
    sk_alphatype_t alphaType;
};

struct sk_rect_t {
    float left;
    float top;
    float right;
    float bottom;
};

struct sk_matrix_t {
    float mat[9];
};

using sk_canvas_t = SkCanvas;
using sk_data_t = SkData;
using sk_image_t = SkImage;
using sk_maskfilter_t = SkMaskFilter;
using sk_paint_t = SkPaint;
using sk_path_t = SkPath;
using sk_picture_t = SkPicture;
using sk_picture_recorder_t = SkPictureRecorder;
using sk_shader_t = SkShader;
using sk_surface_t = SkSurface;

static SkColorType to_color_type(sk_colortype_t ct) {
    switch (ct) {
        case sk_colortype_rgba8888: return kRGBA_8888_SkColorType;
        case sk_colortype_bgra8888: return kBGRA_8888_SkColorType;
        case sk_colortype_alpha8: return kAlpha_8_SkColorType;
        default: return kUnknown_SkColorType;
    }
}

static SkAlphaType to_alpha_type(sk_alphatype_t at) {
    switch (at) {
        case sk_alphatype_premul_alpha: return kPremul_SkAlphaType;
        case sk_alphatype_unpremul_alpha: return kUnpremul_SkAlphaType;
        default: return kOpaque_SkAlphaType;
    }
}

sk_colortype_t sk_colortype_get_default_8888() { return sk_colortype_rgba8888; }

sk_surface_t* sk_surface_new_raster(const sk_imageinfo_t* info) {
    if (!info) return nullptr;
    SkImageInfo ii = SkImageInfo::Make(info->width, info->height, to_color_type(info->colorType), to_alpha_type(info->alphaType));
    return SkSurfaces::Raster(ii).release();
}

sk_surface_t* sk_surface_new_raster_direct(const sk_imageinfo_t* info, void* pixels, size_t rowBytes) {
    if (!info || pixels == nullptr) return nullptr;
    SkImageInfo ii = SkImageInfo::Make(info->width, info->height, to_color_type(info->colorType), to_alpha_type(info->alphaType));
    return SkSurfaces::WrapPixels(ii, pixels, rowBytes).release();
}

void sk_surface_unref(sk_surface_t* surface) { SkSafeUnref(surface); }

sk_canvas_t* sk_surface_get_canvas(sk_surface_t* surface) { return surface ? surface->getCanvas() : nullptr; }

sk_image_t* sk_surface_new_image_snapshot(sk_surface_t* surface) {
    if (!surface) return nullptr;
    return surface->makeImageSnapshot().release();
}

void sk_canvas_draw_paint(sk_canvas_t* canvas, sk_paint_t* paint) {
    if (canvas && paint) canvas->drawPaint(*paint);
}

void sk_canvas_draw_rect(sk_canvas_t* canvas, const sk_rect_t* rect, sk_paint_t* paint) {
    if (canvas && rect && paint) {
        SkRect r = SkRect::MakeLTRB(rect->left, rect->top, rect->right, rect->bottom);
        canvas->drawRect(r, *paint);
    }
}

void sk_canvas_draw_image(sk_canvas_t* canvas, const sk_image_t* image, float x, float y, const sk_paint_t* paint) {
    if (canvas && image) {
        SkSamplingOptions sampling;
        canvas->drawImage(sk_ref_sp(image), x, y, sampling, paint);
    }
}

void sk_canvas_draw_image_rect(sk_canvas_t* canvas, const sk_image_t* image, const sk_rect_t* src, const sk_rect_t* dst, const sk_paint_t* paint) {
    if (canvas && image && src && dst) {
        SkRect s = SkRect::MakeLTRB(src->left, src->top, src->right, src->bottom);
        SkRect d = SkRect::MakeLTRB(dst->left, dst->top, dst->right, dst->bottom);
        SkSamplingOptions sampling;
        canvas->drawImageRect(image, s, d, sampling, paint, SkCanvas::kFast_SrcRectConstraint);
    }
}

void sk_canvas_draw_picture(sk_canvas_t* canvas, const sk_picture_t* picture, const sk_matrix_t* matrix, const sk_paint_t* paint) {
    if (canvas && picture) {
        SkMatrix m;
        if (matrix) {
            m.set9(matrix->mat);
        }
        canvas->drawPicture(sk_ref_sp(picture), matrix ? &m : nullptr, paint);
    }
}

sk_paint_t* sk_paint_new() {
    auto* p = new SkPaint();
    p->setAntiAlias(true);
    return p;
}

void sk_paint_delete(sk_paint_t* paint) { delete paint; }

bool sk_paint_is_antialias(const sk_paint_t* paint) { return paint ? paint->isAntiAlias() : false; }

void sk_paint_set_antialias(sk_paint_t* paint, bool value) {
    if (paint) paint->setAntiAlias(value);
}

sk_color_t sk_paint_get_color(const sk_paint_t* paint) { return paint ? paint->getColor() : 0; }

void sk_paint_set_color(sk_paint_t* paint, sk_color_t color) {
    if (paint) paint->setColor(color);
}

bool sk_paint_is_stroke(const sk_paint_t* paint) { return paint ? paint->getStyle() == SkPaint::kStroke_Style : false; }

void sk_paint_set_stroke(sk_paint_t* paint, bool value) {
    if (paint) paint->setStyle(value ? SkPaint::kStroke_Style : SkPaint::kFill_Style);
}

float sk_paint_get_stroke_width(const sk_paint_t* paint) { return paint ? paint->getStrokeWidth() : 0.0f; }

void sk_paint_set_stroke_width(sk_paint_t* paint, float width) {
    if (paint) paint->setStrokeWidth(width);
}

float sk_paint_get_stroke_miter(const sk_paint_t* paint) { return paint ? paint->getStrokeMiter() : 0.0f; }

void sk_paint_set_stroke_miter(sk_paint_t* paint, float miter) {
    if (paint) paint->setStrokeMiter(miter);
}

int32_t sk_paint_get_stroke_cap(const sk_paint_t* paint) { return paint ? static_cast<int32_t>(paint->getStrokeCap()) : 0; }

void sk_paint_set_stroke_cap(sk_paint_t* paint, int32_t cap) {
    if (paint) paint->setStrokeCap(static_cast<SkPaint::Cap>(cap));
}

int32_t sk_paint_get_stroke_join(const sk_paint_t* paint) { return paint ? static_cast<int32_t>(paint->getStrokeJoin()) : 0; }

void sk_paint_set_stroke_join(sk_paint_t* paint, int32_t join) {
    if (paint) paint->setStrokeJoin(static_cast<SkPaint::Join>(join));
}

void sk_paint_set_shader(sk_paint_t* paint, sk_shader_t* shader) {
    if (paint) paint->setShader(sk_ref_sp(shader));
}

void sk_paint_set_maskfilter(sk_paint_t* paint, sk_maskfilter_t* filter) {
    if (paint) paint->setMaskFilter(sk_ref_sp(filter));
}

sk_data_t* sk_image_encode(const sk_image_t* image) {
    if (!image) return nullptr;
    SkPngEncoder::Options opts;
    return SkPngEncoder::Encode(nullptr, image, opts).release();
}

void sk_image_ref(const sk_image_t* image) { SkSafeRef(image); }

void sk_image_unref(const sk_image_t* image) { SkSafeUnref(image); }

int sk_image_get_width(const sk_image_t* image) { return image ? image->width() : 0; }

int sk_image_get_height(const sk_image_t* image) { return image ? image->height() : 0; }

uint32_t sk_image_get_unique_id(const sk_image_t* image) { return image ? image->uniqueID() : 0; }

void sk_data_ref(const sk_data_t* data) { SkSafeRef(data); }

void sk_data_unref(const sk_data_t* data) { SkSafeUnref(data); }

size_t sk_data_get_size(const sk_data_t* data) { return data ? data->size() : 0; }

const void* sk_data_get_data(const sk_data_t* data) { return data ? data->data() : nullptr; }

// Metal support - CPU raster surface with Metal present
#ifdef __APPLE__

// Simplified: Use CPU raster surface, present via Metal
struct MetalRasterContext {
    void* device;
    void* queue;
    void* layer;
};

using gr_direct_context_t = MetalRasterContext;

gr_direct_context_t* gr_direct_context_make_metal(void* device, void* queue) {
    if (!device || !queue) return nullptr;
    
    auto* ctx = new MetalRasterContext();
    ctx->device = (__bridge_retained void*)device;
    ctx->queue = (__bridge_retained void*)queue;
    ctx->layer = nullptr;
    return ctx;
}

void gr_direct_context_unref(gr_direct_context_t* context) {
    if (!context) return;
    if (context->device) CFRelease((CFTypeRef)context->device);
    if (context->queue) CFRelease((CFTypeRef)context->queue);
    delete context;
}

void gr_direct_context_flush_and_submit(gr_direct_context_t* context, bool sync) {
    // No-op for CPU raster
    (void)context;
    (void)sync;
}

sk_surface_t* sk_surface_wrap_metal_layer(gr_direct_context_t* context, void* layer, int width, int height) {
    if (!context || !layer) return nullptr;
    
    // Create CPU raster surface
    SkImageInfo info = SkImageInfo::Make(width, height, kBGRA_8888_SkColorType, kPremul_SkAlphaType);
    auto surface = SkSurfaces::Raster(info);
    
    // Store layer for present
    context->layer = layer;
    
    return surface.release();
}

void sk_surface_flush_and_submit(sk_surface_t* surface) {
    // No-op for CPU raster - pixels are already in surface
    (void)surface;
}

// Simple text drawing (for FPS counter)
void sk_canvas_draw_simple_text(sk_canvas_t* canvas, const char* text, size_t len, float x, float y, const sk_paint_t* paint) {
    if (!canvas || !text || !paint) return;
    
    SkFont font;
    font.setSize(14.0f);
    
    canvas->drawSimpleText(text, len, SkTextEncoding::kUTF8, x, y, font, *paint);
}

// Present CPU raster surface to Metal layer
void sk_surface_present_to_layer(gr_direct_context_t* context, sk_surface_t* surface, void* layer) {
    if (!context || !surface || !layer) return;
    
    CAMetalLayer* metalLayer = (__bridge CAMetalLayer*)layer;
    
    // Get drawable
    id<CAMetalDrawable> drawable = [metalLayer nextDrawable];
    if (!drawable) return;
    
    // Get surface pixels
    SkPixmap pixmap;
    if (!surface->peekPixels(&pixmap)) return;
    
    // Copy pixels to Metal texture
    id<MTLTexture> texture = drawable.texture;
    MTLRegion region = MTLRegionMake2D(0, 0, pixmap.width(), pixmap.height());
    [texture replaceRegion:region 
                mipmapLevel:0 
                  withBytes:pixmap.addr() 
                bytesPerRow:pixmap.rowBytes()];
    
    // Present using command queue
    id<MTLCommandQueue> queue = (__bridge id<MTLCommandQueue>)context->queue;
    if (!queue) return;
    id<MTLCommandBuffer> commandBuffer = [queue commandBuffer];
    [commandBuffer presentDrawable:drawable];
    [commandBuffer commit];
}
#endif

}
