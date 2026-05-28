package dev.substrate.semantic

data class SemanticElement(
    val id: String,
    val platformId: String?,
    val type: String,
    val content: String?,
    val font: FontInfo?,
    val color: String?,
    val bounds: Bounds,
    val zIndex: Int,
    val clickable: Boolean,
    val enabled: Boolean,
    val accessible: Boolean,
    val a11yLabel: String?,
    val a11yId: String?,
    val background: String?,
    val cornerRadius: Float?,
    val padding: PaddingInfo?,
    val margin: MarginInfo?,
    val elevation: Float?,
    val render: String?,
    val lineCount: Int? = null,
    val truncated: Boolean? = null,
    val imageResource: String? = null,
    val imageType: String? = null,
    val imagePath: String? = null,
    val border: BorderInfo? = null,
    val gradient: GradientInfo? = null,
    val tapTarget: Bounds? = null,
)

data class Bounds(val x: Int, val y: Int, val w: Int, val h: Int)

data class BorderInfo(val width: Float, val color: String?)

data class GradientInfo(val type: String, val colors: List<String>, val orientation: String?)

data class FontInfo(val family: String, val weight: String, val size: Float)

data class PaddingInfo(val top: Int, val bottom: Int, val start: Int, val end: Int)

data class MarginInfo(val top: Int, val bottom: Int, val start: Int, val end: Int)

data class ViewportInfo(val width: Int, val height: Int, val density: Float)

data class ScrollCaptureInfo(
    val scrollView: Bounds,
    val advancePx: Int,
    val steps: Int,
    val stepOffsets: List<Int>,
)

data class SemanticSchema(
    val screen: String,
    val device: String,
    val platform: String,
    val timestamp: String,
    val viewport: ViewportInfo,
    val elements: List<SemanticElement>,
    val scrollCapture: ScrollCaptureInfo? = null,
)
