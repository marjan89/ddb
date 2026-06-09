package dev.substrate.semantic

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Rect
import android.graphics.Typeface
import android.graphics.drawable.BitmapDrawable
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.graphics.drawable.VectorDrawable
import android.os.Build
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.EditText
import android.widget.ImageView
import android.widget.TextView
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

object ViewTreeWalker {
    @Volatile
    var lastDebugLog: String = ""
        private set

    private val fontFingerprints = mutableMapOf<Long, String>()
    private val glyphHashCache = mutableMapOf<Int, Long>()

    fun initFontMap(context: android.content.Context) {
        fontFingerprints.clear()
        glyphHashCache.clear()
        try {
            val fontClass = Class.forName("${context.packageName}.R\$font")
            val fields = fontClass.declaredFields
            android.util.Log.i("SemanticAgent", "R.font class found with ${fields.size} fields")
            for (field in fields) {
                if (field.type != Int::class.javaPrimitiveType) continue
                try {
                    val resId = field.getInt(null)
                    val typeface =
                        if (Build.VERSION.SDK_INT >= 26) {
                            context.resources.getFont(resId)
                        } else {
                            null
                        }
                    if (typeface != null) {
                        val hash = computeGlyphHash(typeface)
                        fontFingerprints[hash] = field.name
                        android.util.Log.d("SemanticAgent", "font indexed: ${field.name} hash=$hash")
                    }
                } catch (e: Exception) {
                    android.util.Log.w("SemanticAgent", "font ${field.name} load failed: ${e.message}")
                }
            }
            android.util.Log.i("SemanticAgent", "font fingerprints: ${fontFingerprints.size} fonts indexed")
        } catch (_: ClassNotFoundException) {
            android.util.Log.i("SemanticAgent", "no R.font class found — font fingerprinting disabled")
        } catch (e: Exception) {
            android.util.Log.e("SemanticAgent", "font init failed: ${e.message}", e)
        }
    }

    private fun computeGlyphHash(typeface: Typeface): Long {
        val paint =
            android.graphics.Paint().apply {
                this.typeface = typeface
                textSize = 48f
                isAntiAlias = false
            }
        val text = "HgQW"
        val w = paint.measureText(text).toInt().coerceAtLeast(1)
        val h = 64
        val bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ALPHA_8)
        val canvas = Canvas(bmp)
        canvas.drawText(text, 0f, 48f, paint)
        val pixels = IntArray(w * h)
        bmp.getPixels(pixels, 0, w, 0, 0, w, h)
        bmp.recycle()
        var hash = -3750763034362895579L // 0xcbf29ce484222325
        for (p in pixels) {
            hash = hash xor p.toLong()
            hash *= 1099511628211L // 0x100000001b3
        }
        return hash
    }

    private fun getGlyphHash(typeface: Typeface): Long {
        val id = System.identityHashCode(typeface)
        return glyphHashCache.getOrPut(id) { computeGlyphHash(typeface) }
    }

    fun walk(activity: Activity): SemanticSchema {
        val density = activity.resources.displayMetrics.density
        val decorView = activity.window.decorView
        val elements = mutableListOf<SemanticElement>()
        val log = StringBuilder()
        imageOutputDir = java.io.File(activity.cacheDir, "vdb-images").apply { mkdirs() }
        var z = 0

        fun nextZ(): Int = z++

        fun walkView(
            view: View,
            parentExternal: Boolean,
            parentHidden: Boolean,
        ) {
            val resId = extractResourceId(view)
            val tag = resId ?: view.javaClass.simpleName

            if (view.visibility == View.GONE) {
                log.appendLine("SKIP $tag reason=gone")
                return
            }

            val isGroup = view is ViewGroup
            val childCount = if (isGroup) (view as ViewGroup).childCount else 0
            val effectivelyHidden =
                parentHidden ||
                    (view.visibility == View.INVISIBLE) ||
                    (view.alpha == 0f)

            if (effectivelyHidden && childCount == 0) {
                log.appendLine("SKIP $tag reason=hidden_leaf hidden=$parentHidden vis=${view.visibility} alpha=${view.alpha}")
                return
            }

            val rect = Rect()
            if (!view.getGlobalVisibleRect(rect)) {
                log.appendLine("SKIP $tag reason=no_visible_rect")
                return
            }

            val bounds =
                Bounds(
                    x = rect.left,
                    y = rect.top,
                    w = rect.right - rect.left,
                    h = rect.bottom - rect.top,
                )
            if (bounds.w <= 0 || bounds.h <= 0) {
                log.appendLine("SKIP $tag reason=zero_size bounds=$bounds")
                return
            }

            val isExternal = isExternalSurface(view)
            val effectiveExternal = isExternal || parentExternal

            if (effectivelyHidden) {
                val element = buildElement(view, bounds, density, nextZ(), effectiveExternal)
                elements.add(element)
                log.appendLine(
                    "EMIT $tag z=${element.zIndex} type=${element.type} bounds=$bounds hidden_container=true vis=${view.visibility} alpha=${view.alpha} children=$childCount",
                )
                if (!isExternal && isGroup) {
                    val group = view as ViewGroup
                    for (i in 0 until group.childCount) {
                        walkView(group.getChildAt(i), effectiveExternal, true)
                    }
                }
                return
            }

            val element = buildElement(view, bounds, density, nextZ(), effectiveExternal)
            elements.add(element)
            val c = element.content?.take(30) ?: "null"
            log.appendLine(
                "EMIT $tag z=${element.zIndex} type=${element.type}" +
                    " bounds=$bounds vis=V alpha=${view.alpha}" +
                    " accessible=${view.isImportantForAccessibility}" +
                    " children=$childCount clickable=${view.isClickable}" +
                    " content=$c",
            )

            if (isExternal) return

            if (isComposeView(view)) {
                try {
                    val before = elements.size
                    val ok = walkComposeSemanticsOwner(view, density, ::nextZ, elements, log)
                    if (!ok && view is ViewGroup) {
                        val group = view as ViewGroup
                        for (i in 0 until group.childCount) {
                            walkView(group.getChildAt(i), effectiveExternal, false)
                        }
                    }
                    log.appendLine("COMPOSE_DESCENT root=$tag ok=$ok emitted=${elements.size - before}")
                } catch (e: Throwable) {
                    log.appendLine("COMPOSE_DESCENT root=$tag failed=${e.javaClass.simpleName}:${e.message}")
                }
                return
            }

            if (isGroup) {
                val group = view as ViewGroup
                for (i in 0 until group.childCount) {
                    walkView(group.getChildAt(i), effectiveExternal, false)
                }
            }
        }

        walkView(decorView, parentExternal = false, parentHidden = false)
        removeGhostTouchTargets(elements, log)
        disambiguateIds(elements)
        lastDebugLog = log.toString()

        val sdf = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.US)
        sdf.timeZone = TimeZone.getTimeZone("UTC")

        val metrics = activity.resources.displayMetrics
        val viewport =
            ViewportInfo(
                width = metrics.widthPixels,
                height = metrics.heightPixels,
                density = density,
            )

        return SemanticSchema(
            screen = activity.javaClass.simpleName,
            device = Build.MODEL,
            platform = "android",
            timestamp = sdf.format(Date()),
            viewport = viewport,
            elements = elements,
        )
    }

    private fun removeGhostTouchTargets(
        elements: MutableList<SemanticElement>,
        log: StringBuilder,
    ) {
        val toRemove = mutableSetOf<Int>()
        for (i in elements.indices) {
            val a = elements[i]
            if (!a.clickable) continue
            if (a.type != "button" && a.type != "view") continue
            if (!a.content.isNullOrEmpty()) continue
            if (a.background != null) continue

            for (j in elements.indices) {
                if (i == j) continue
                val b = elements[j]
                if (!b.content.isNullOrEmpty() && boundsContains(a.bounds, b.bounds)) {
                    toRemove.add(i)
                    log.appendLine("GHOST_FILTER remove id=${a.id} z=${a.zIndex} — contains ${b.id} with content")
                    break
                }
            }
        }
        for (idx in toRemove.sortedDescending()) {
            elements.removeAt(idx)
        }
    }

    private var imageOutputDir: java.io.File? = null

    private fun buildElement(
        view: View,
        bounds: Bounds,
        density: Float,
        z: Int,
        isExternal: Boolean,
    ): SemanticElement {
        val resourceId = extractResourceId(view)
        val content: String?
        var font: FontInfo? = null
        var textColor: String? = null
        var lineCount: Int? = null
        var truncated: Boolean? = null
        var imageResource: String? = null
        var imageType: String? = null
        var imagePath: String? = null
        val type: String

        when (view) {
            is EditText -> {
                content = view.text?.toString()?.ifEmpty { view.hint?.toString() }
                font = extractFont(view, density)
                textColor = colorToHex(view.currentTextColor)
                type = "input"
            }

            is TextView -> {
                content = view.text?.toString()
                font = extractFont(view, density)
                textColor = colorToHex(view.currentTextColor)
                lineCount = view.lineCount.takeIf { it > 0 }
                truncated =
                    view.layout?.let { layout ->
                        val lastLine = layout.lineCount - 1
                        lastLine >= 0 && layout.getEllipsisCount(lastLine) > 0
                    }
                type = if (view.isClickable) "button" else "text"
            }

            is ImageView -> {
                content = view.contentDescription?.toString()
                textColor = view.imageTintList?.defaultColor?.let { colorToHex(it) }
                val imgInfo = extractImageInfo(view, resourceId ?: "image_${bounds.x}_${bounds.y}")
                imageResource = imgInfo?.first
                imageType = imgInfo?.second
                imagePath = imgInfo?.third
                type = "image"
            }

            else -> {
                content = view.contentDescription?.toString()
                type =
                    when {
                        view.isClickable -> "button"
                        view is ViewGroup -> "container"
                        else -> "view"
                    }
            }
        }

        val id =
            when {
                !content.isNullOrEmpty() -> slugify(content)
                resourceId != null -> resourceId.lowercase()
                else -> "${view.javaClass.simpleName.lowercase()}_${bounds.x}_${bounds.y}"
            }

        val padding =
            if (view.paddingTop > 0 || view.paddingBottom > 0 ||
                view.paddingStart > 0 || view.paddingEnd > 0
            ) {
                PaddingInfo(
                    top = (view.paddingTop / density).toInt(),
                    bottom = (view.paddingBottom / density).toInt(),
                    start = (view.paddingStart / density).toInt(),
                    end = (view.paddingEnd / density).toInt(),
                )
            } else {
                null
            }

        val margin =
            (view.layoutParams as? ViewGroup.MarginLayoutParams)?.let { lp ->
                if (lp.topMargin > 0 || lp.bottomMargin > 0 ||
                    lp.marginStart > 0 || lp.marginEnd > 0
                ) {
                    MarginInfo(
                        top = (lp.topMargin / density).toInt(),
                        bottom = (lp.bottomMargin / density).toInt(),
                        start = (lp.marginStart / density).toInt(),
                        end = (lp.marginEnd / density).toInt(),
                    )
                } else {
                    null
                }
            }

        val elev = view.elevation + view.translationZ
        val elevDp = if (elev > 0f) elev / density else null

        return SemanticElement(
            id = id,
            platformId = resourceId,
            type = type,
            content = content,
            font = font,
            color = textColor,
            bounds = bounds,
            zIndex = z,
            clickable = view.isClickable || hasClickableAncestor(view),
            enabled = view.isEnabled,
            accessible = view.isImportantForAccessibility,
            a11yLabel = view.contentDescription?.toString(),
            a11yId = resourceId,
            background = extractBackgroundColor(view),
            cornerRadius = extractCornerRadius(view)?.let { it / density },
            padding = padding,
            margin = margin,
            elevation = elevDp,
            render = if (isExternal) "external" else null,
            lineCount = lineCount,
            truncated = truncated,
            imageResource = imageResource,
            imageType = imageType,
            imagePath = imagePath,
            border = extractBorder(view, density),
            gradient = extractGradient(view),
            tapTarget = if (!view.isClickable && !view.isLongClickable) findClickableAncestorBounds(view) else null,
        )
    }

    /**
     * Walks Compose semantics via reflection on AndroidComposeView.semanticsOwner.
     * No accessibility flags toggled, no AccessibilityNodeProvider — pure introspection
     * of Compose's internal SemanticsNode tree. Returns true if walk succeeded.
     */
    private fun walkComposeSemanticsOwner(
        view: View,
        density: Float,
        nextZ: () -> Int,
        out: MutableList<SemanticElement>,
        log: StringBuilder,
    ): Boolean {
        // Find AndroidComposeView (may be `view` itself or a descendant)
        val androidComposeView = findAndroidComposeView(view) ?: return false
        val owner = try {
            val m = androidComposeView.javaClass.methods.firstOrNull { it.name == "getSemanticsOwner" && it.parameterCount == 0 }
            m?.invoke(androidComposeView)
        } catch (e: Throwable) {
            log.appendLine("COMPOSE_SEM owner fetch failed=${e.javaClass.simpleName}:${e.message}")
            null
        } ?: return false

        val rootNode = try {
            val m = owner.javaClass.methods.firstOrNull { it.name == "getUnmergedRootSemanticsNode" && it.parameterCount == 0 }
                ?: owner.javaClass.methods.firstOrNull { it.name == "getRootSemanticsNode" && it.parameterCount == 0 }
            m?.invoke(owner)
        } catch (e: Throwable) {
            log.appendLine("COMPOSE_SEM root fetch failed=${e.javaClass.simpleName}:${e.message}")
            null
        } ?: return false

        walkSemanticsNode(rootNode, density, nextZ, out, log, 0, false)
        return true
    }

    private fun findAndroidComposeView(view: View): View? {
        if (view.javaClass.name == "androidx.compose.ui.platform.AndroidComposeView") return view
        if (view is ViewGroup) {
            for (i in 0 until view.childCount) {
                val r = findAndroidComposeView(view.getChildAt(i))
                if (r != null) return r
            }
        }
        return null
    }

    private fun walkSemanticsNode(
        node: Any,
        density: Float,
        nextZ: () -> Int,
        out: MutableList<SemanticElement>,
        log: StringBuilder,
        depth: Int,
        inheritedClickable: Boolean,
    ) {
        val cls = node.javaClass
        // SemanticsNode.boundsInWindow: Rect (composeui Rect, not graphics Rect)
        val composeRect = try {
            val m = cls.methods.firstOrNull { it.name == "getBoundsInWindow" && it.parameterCount == 0 }
            m?.invoke(node)
        } catch (_: Throwable) { null }
        val bounds = composeRectToBounds(composeRect)

        // SemanticsNode.config: SemanticsConfiguration
        val config = try {
            val m = cls.methods.firstOrNull { it.name == "getConfig" && it.parameterCount == 0 }
            m?.invoke(node)
        } catch (_: Throwable) { null }

        val text = extractSemanticsText(config)
        val contentDesc = extractSemanticsContentDesc(config)
        val role = extractSemanticsRole(config)
        val ownClick = hasSemanticsAction(config, "OnClick")
        // TD-59: Compose Modifier.clickable installs the OnClick semantics even
        // when `enabled=false`; the action body is gated separately. Detect the
        // Disabled semantics key (set by Material/Modifier when enabled=false)
        // and treat as non-clickable regardless of OnClick presence.
        val isDisabled = hasSemanticsKey(config, "Disabled")
        val effectiveOwnClick = ownClick && !isDisabled
        // In Compose's unmerged tree the clickable Box and its inner Text label
        // are separate nodes. Propagate clickable from a clickable ancestor so
        // text leaves inside a Button report clickable=true. Disabled blocks
        // propagation locally and downstream.
        val isClickable = effectiveOwnClick || (inheritedClickable && !isDisabled)
        val isInput = hasSemanticsKey(config, "EditableText") || hasSemanticsAction(config, "SetText")

        val content = text ?: contentDesc
        if (bounds != null && (content != null || isClickable || isInput || role != null)) {
            val type = when {
                isInput -> "input"
                isClickable && !content.isNullOrEmpty() -> "button"
                isClickable -> "button"
                !content.isNullOrEmpty() -> "text"
                else -> "view"
            }
            val id = if (!content.isNullOrEmpty()) slugify(content)
                else "compose_${role?.lowercase() ?: "node"}_${bounds.x}_${bounds.y}"
            val z = nextZ()
            out.add(
                SemanticElement(
                    id = id, platformId = null, type = type, content = content,
                    font = null, color = null, bounds = bounds, zIndex = z,
                    clickable = isClickable, enabled = !isDisabled, accessible = true,
                    a11yLabel = contentDesc, a11yId = null, background = null,
                    cornerRadius = null, padding = null, margin = null, elevation = null,
                    render = null, lineCount = null, truncated = null,
                    imageResource = null, imageType = null, imagePath = null,
                    border = null, gradient = null, tapTarget = null,
                ),
            )
            log.appendLine("COMPOSE_SEM_EMIT z=$z type=$type bounds=$bounds clickable=$isClickable content=${content?.take(30) ?: "null"}")
        }

        // Recurse: SemanticsNode.children: List<SemanticsNode>
        val children = try {
            // try getChildren() first (returns List); fallback to getReplacedChildren()
            val m = cls.methods.firstOrNull { it.name == "getChildren" && it.parameterCount == 0 }
                ?: cls.methods.firstOrNull { it.name == "getReplacedChildren" && it.parameterCount == 0 }
            m?.invoke(node) as? List<*>
        } catch (_: Throwable) { null }
        if (children != null) {
            for (child in children) {
                if (child != null) walkSemanticsNode(child, density, nextZ, out, log, depth + 1, isClickable)
            }
        }
    }

    private fun composeRectToBounds(rect: Any?): Bounds? {
        if (rect == null) return null
        return try {
            val cls = rect.javaClass
            val left = (cls.getMethod("getLeft").invoke(rect) as Float)
            val top = (cls.getMethod("getTop").invoke(rect) as Float)
            val right = (cls.getMethod("getRight").invoke(rect) as Float)
            val bottom = (cls.getMethod("getBottom").invoke(rect) as Float)
            val w = (right - left).toInt()
            val h = (bottom - top).toInt()
            if (w <= 0 || h <= 0) null
            else Bounds(left.toInt(), top.toInt(), w, h)
        } catch (_: Throwable) { null }
    }

    private fun extractSemanticsText(config: Any?): String? {
        if (config == null) return null
        // SemanticsConfiguration is iterable over Map.Entry<SemanticsPropertyKey<*>, Any?>
        try {
            val iter = (config as? Iterable<*>)?.iterator() ?: return null
            while (iter.hasNext()) {
                val entry = iter.next() ?: continue
                val key = entry.javaClass.getMethod("getKey").invoke(entry)
                val keyName = key?.javaClass?.getMethod("getName")?.invoke(key) as? String
                if (keyName == "Text" || keyName == "EditableText") {
                    val value = entry.javaClass.getMethod("getValue").invoke(entry)
                    // Text is List<AnnotatedString>, EditableText is AnnotatedString
                    val str = when (value) {
                        is List<*> -> value.joinToString(" ") { it?.toString() ?: "" }
                        else -> value?.toString()
                    }
                    if (!str.isNullOrEmpty()) return str
                }
            }
        } catch (_: Throwable) {}
        return null
    }

    private fun extractSemanticsContentDesc(config: Any?): String? {
        if (config == null) return null
        try {
            val iter = (config as? Iterable<*>)?.iterator() ?: return null
            while (iter.hasNext()) {
                val entry = iter.next() ?: continue
                val key = entry.javaClass.getMethod("getKey").invoke(entry)
                val keyName = key?.javaClass?.getMethod("getName")?.invoke(key) as? String
                if (keyName == "ContentDescription") {
                    val value = entry.javaClass.getMethod("getValue").invoke(entry)
                    val str = when (value) {
                        is List<*> -> value.joinToString(" ") { it?.toString() ?: "" }
                        else -> value?.toString()
                    }
                    if (!str.isNullOrEmpty()) return str
                }
            }
        } catch (_: Throwable) {}
        return null
    }

    private fun extractSemanticsRole(config: Any?): String? {
        if (config == null) return null
        try {
            val iter = (config as? Iterable<*>)?.iterator() ?: return null
            while (iter.hasNext()) {
                val entry = iter.next() ?: continue
                val key = entry.javaClass.getMethod("getKey").invoke(entry)
                val keyName = key?.javaClass?.getMethod("getName")?.invoke(key) as? String
                if (keyName == "Role") {
                    return entry.javaClass.getMethod("getValue").invoke(entry)?.toString()
                }
            }
        } catch (_: Throwable) {}
        return null
    }

    private fun hasSemanticsAction(config: Any?, actionName: String): Boolean {
        if (config == null) return false
        try {
            val iter = (config as? Iterable<*>)?.iterator() ?: return false
            while (iter.hasNext()) {
                val entry = iter.next() ?: continue
                val key = entry.javaClass.getMethod("getKey").invoke(entry)
                val keyName = key?.javaClass?.getMethod("getName")?.invoke(key) as? String
                if (keyName == actionName) return true
            }
        } catch (_: Throwable) {}
        return false
    }

    private fun hasSemanticsKey(config: Any?, keyName: String): Boolean = hasSemanticsAction(config, keyName)

    private fun isComposeView(view: View): Boolean {
        var c: Class<*>? = view.javaClass
        while (c != null) {
            val n = c.name
            if (n == "androidx.compose.ui.platform.AbstractComposeView" ||
                n == "androidx.compose.ui.platform.AndroidComposeView"
            ) return true
            c = c.superclass
        }
        return false
    }

    private fun composeA11yDelegate(view: View): Any? {
        var c: Class<*>? = view.javaClass
        while (c != null) {
            if (c.name == "androidx.compose.ui.platform.AndroidComposeView") {
                for (fieldName in listOf("composeAccessibilityDelegate", "accessibilityDelegate")) {
                    try {
                        val f = c.getDeclaredField(fieldName)
                        f.isAccessible = true
                        return f.get(view) ?: continue
                    } catch (_: NoSuchFieldException) {}
                }
                return null
            }
            c = c.superclass
        }
        return null
    }

    private fun setComposeA11yFlag(delegate: Any?, enabled: Boolean) {
        if (delegate == null) return
        try {
            val f = delegate.javaClass.getDeclaredField("accessibilityForceEnabledForTesting")
            f.isAccessible = true
            f.setBoolean(delegate, enabled)
        } catch (_: Throwable) {}
    }

    private fun walkComposeProvider(
        node: AccessibilityNodeInfo,
        provider: android.view.accessibility.AccessibilityNodeProvider,
        density: Float,
        nextZ: () -> Int,
        out: MutableList<SemanticElement>,
        log: StringBuilder,
    ) {
        val childCount = node.childCount
        for (i in 0 until childCount) {
            // Compose's createAccessibilityNodeInfo returns unsealed nodes; getChild() throws
            // "Cannot perform this action on a not sealed instance". The hidden API
            // AccessibilityNodeInfo.getChildId(int) returns the packed (sourceId,virtualId)
            // long; we extract the virtual ID and re-query the provider.
            val packed: Long = try {
                var p: Long = 0L
                var found = false
                val cls = AccessibilityNodeInfo::class.java
                try {
                    val f = cls.getDeclaredField("mChildNodeIds")
                    f.isAccessible = true
                    val arr = f.get(node)
                    if (arr is LongArray && i < arr.size) {
                        p = arr[i]; found = true
                    } else if (arr != null) {
                        val getM = arr.javaClass.getMethod("get", Int::class.javaPrimitiveType)
                        p = getM.invoke(arr, i) as Long; found = true
                    }
                } catch (_: Throwable) {}
                if (!found) for (m in cls.declaredMethods) {
                    if (m.name == "getChildId" && m.parameterTypes.size == 1) {
                        m.isAccessible = true; p = m.invoke(node, i) as Long; found = true; break
                    }
                }
                if (!found) throw NoSuchMethodException("no getChildId or mChildNodeIds")
                p
            } catch (e: Throwable) {
                log.appendLine("COMPOSE_CHILDID_ERR i=$i err=${e.javaClass.simpleName}:${e.message}")
                continue
            }
            // Try both halves and the raw value to handle differing pack orders.
            val low = (packed and 0xFFFFFFFFL).toInt()
            val high = (packed ushr 32).toInt()
            val candidates = listOf(low, high, packed.toInt())
            var child: AccessibilityNodeInfo? = null
            var hitVid = -1
            for (vid in candidates.distinct()) {
                val c = try { provider.createAccessibilityNodeInfo(vid) } catch (_: Throwable) { null }
                if (c != null) { child = c; hitVid = vid; break }
            }
            if (child == null) {
                log.appendLine("COMPOSE_CHILD_NULL i=$i packed=$packed low=$low high=$high")
                continue
            }
            val virtualId = hitVid
            try {
                val rect = Rect()
                child.getBoundsInScreen(rect)
                val text = child.text?.toString()
                val desc = child.contentDescription?.toString()
                log.appendLine(
                    "COMPOSE_CHILD i=$i class=${child.className} bounds=[${rect.left},${rect.top},${rect.right},${rect.bottom}] " +
                        "text=${text?.take(40) ?: "null"} desc=${desc?.take(40) ?: "null"} children=${child.childCount}",
                )
                val element = buildElementFromA11y(child, density, nextZ())
                if (element != null) {
                    out.add(element)
                    log.appendLine(
                        "COMPOSE_EMIT z=${element.zIndex} type=${element.type} bounds=${element.bounds} " +
                            "clickable=${element.clickable} content=${element.content?.take(30) ?: "null"}",
                    )
                }
                walkComposeProvider(child, provider, density, nextZ, out, log)
            } finally {
                try { child.recycle() } catch (_: Throwable) {}
            }
        }
    }

    private fun buildElementFromA11y(
        info: AccessibilityNodeInfo,
        density: Float,
        z: Int,
    ): SemanticElement? {
        val rect = Rect()
        info.getBoundsInScreen(rect)
        if (rect.width() <= 0 || rect.height() <= 0) return null

        val text = info.text?.toString()
        val contentDesc = info.contentDescription?.toString()
        val hint = if (Build.VERSION.SDK_INT >= 26) info.hintText?.toString() else null
        val content = text ?: contentDesc ?: hint

        val className = info.className?.toString() ?: ""
        val isInput = className.endsWith("EditText") || info.isEditable
        // TD-59: Compose Button doesn't set the legacy info.isClickable bit;
        // it publishes click affordance via actionList ACTION_CLICK and/or a
        // "Button" role. Treat either as clickable so element_state assertions
        // toggle correctly with the enabled= parameter.
        val hasClickAction = try {
            info.actionList.any { it.id == AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK.id }
        } catch (_: Throwable) { false }
        val isComposeButton = className.endsWith("Button") ||
            className == "android.view.View" && hasClickAction
        val clickable = info.isClickable || hasClickAction || isComposeButton
        val type = when {
            isInput -> "input"
            clickable && !content.isNullOrEmpty() -> "button"
            clickable -> "button"
            !content.isNullOrEmpty() -> "text"
            else -> "view"
        }

        val id = when {
            !content.isNullOrEmpty() -> slugify(content)
            else -> "compose_${className.substringAfterLast('.').lowercase()}_${rect.left}_${rect.top}"
        }

        return SemanticElement(
            id = id,
            platformId = null,
            type = type,
            content = content,
            font = null,
            color = null,
            bounds = Bounds(rect.left, rect.top, rect.width(), rect.height()),
            zIndex = z,
            clickable = clickable,
            enabled = info.isEnabled,
            accessible = info.isImportantForAccessibility,
            a11yLabel = contentDesc,
            a11yId = null,
            background = null,
            cornerRadius = null,
            padding = null,
            margin = null,
            elevation = null,
            render = null,
            lineCount = null,
            truncated = null,
            imageResource = null,
            imageType = null,
            imagePath = null,
            border = null,
            gradient = null,
            tapTarget = null,
        )
    }

    internal fun disambiguateIds(elements: MutableList<SemanticElement>) {
        val counts = mutableMapOf<String, Int>()
        for (e in elements) counts[e.id] = (counts[e.id] ?: 0) + 1

        val seen = mutableMapOf<String, Int>()
        for (i in elements.indices) {
            val e = elements[i]
            if ((counts[e.id] ?: 0) > 1) {
                val idx = seen.getOrDefault(e.id, 0)
                seen[e.id] = idx + 1
                elements[i] = e.copy(id = "${e.id}_$idx")
            }
        }
    }

    private fun extractFont(
        tv: TextView,
        density: Float,
    ): FontInfo {
        val typeface = tv.typeface ?: Typeface.DEFAULT
        val weight =
            if (Build.VERSION.SDK_INT >= 28) {
                typeface.weight
            } else {
                if (typeface.isBold) 700 else 400
            }
        val weightName =
            when {
                weight < 200 -> "thin"
                weight < 300 -> "extralight"
                weight < 400 -> "light"
                weight < 500 -> "regular"
                weight < 600 -> "medium"
                weight < 700 -> "semibold"
                weight < 800 -> "bold"
                weight < 900 -> "extrabold"
                else -> "black"
            }

        val fingerprintMatch = lookupFontByFingerprint(typeface)
        val family = fingerprintMatch?.first ?: extractFontFamily(tv)
        val resolvedWeight = fingerprintMatch?.second ?: weightName

        return FontInfo(
            family = family,
            weight = resolvedWeight,
            size = tv.textSize / density,
        )
    }

    private fun lookupFontByFingerprint(typeface: Typeface): Pair<String, String?>? {
        if (fontFingerprints.isEmpty()) return null
        val hash = getGlyphHash(typeface)
        val resName = fontFingerprints[hash] ?: return null
        val family = cleanFontFamily(resName)
        val weightFromName = extractWeightFromName(resName)
        return Pair(family, weightFromName)
    }

    private val weightSuffixes =
        listOf(
            "_extra_bold" to "extrabold",
            "_semi_bold" to "semibold",
            "_semibold" to "semibold",
            "_bold" to "bold",
            "_medium" to "medium",
            "_regular" to "regular",
            "_light" to "light",
            "_extra_light" to "extralight",
            "_thin" to "thin",
            "_black" to "black",
        )

    private fun extractWeightFromName(name: String): String? {
        var lower = name.lowercase()
        if (lower.endsWith("_italic")) lower = lower.removeSuffix("_italic")
        for ((suffix, weight) in weightSuffixes) {
            if (lower.endsWith(suffix)) return weight
        }
        return null
    }

    private fun extractFontFamily(tv: TextView): String {
        try {
            val attrs = intArrayOf(android.R.attr.fontFamily)
            val ta = tv.context.obtainStyledAttributes(null, attrs, android.R.attr.textViewStyle, 0)
            val family = ta.getString(0)
            ta.recycle()
            if (!family.isNullOrEmpty()) return cleanFontFamily(family)
        } catch (_: Exception) {
        }

        try {
            val field = Typeface::class.java.getDeclaredField("mFamilyName")
            field.isAccessible = true
            val name = field.get(tv.typeface) as? String
            if (name != null && name != "sans-serif") return cleanFontFamily(name)
        } catch (_: Exception) {
        }

        return "sans-serif"
    }

    private fun cleanFontFamily(raw: String): String {
        var name = raw.lowercase()
        if (name.startsWith("res/font/")) name = name.removePrefix("res/font/")
        if (name.startsWith("@font/")) name = name.removePrefix("@font/")
        if (name.endsWith(".ttf") || name.endsWith(".otf")) name = name.substringBeforeLast('.')
        if (name.endsWith("_italic")) name = name.removeSuffix("_italic")
        for (suffix in listOf(
            "_extra_bold",
            "_semi_bold",
            "_semibold",
            "_bold",
            "_medium",
            "_regular",
            "_extra_light",
            "_light",
            "_thin",
            "_black",
        )) {
            if (name.endsWith(suffix)) return name.removeSuffix(suffix)
        }
        return name
    }

    private fun extractBackgroundColor(view: View): String? = extractDrawableColor(view.background)

    private fun extractDrawableColor(drawable: android.graphics.drawable.Drawable?): String? =
        when (drawable) {
            is ColorDrawable -> {
                colorToHex(drawable.color)
            }

            is GradientDrawable -> {
                try {
                    val field = GradientDrawable::class.java.getDeclaredField("mSolidColors")
                    field.isAccessible = true
                    (field.get(drawable) as? android.content.res.ColorStateList)?.defaultColor?.let { colorToHex(it) }
                } catch (_: Exception) {
                    null
                }
            }

            is RippleDrawable -> {
                if (drawable.numberOfLayers > 0) {
                    extractDrawableColor(drawable.getDrawable(0))
                } else {
                    null
                }
            }

            else -> {
                null
            }
        }

    private fun extractImageInfo(
        view: ImageView,
        elementId: String,
    ): Triple<String, String, String>? {
        val drawable = view.drawable ?: return null

        val resName =
            try {
                var cls: Class<*> = view.javaClass
                var field: java.lang.reflect.Field? = null
                while (cls != Any::class.java) {
                    try {
                        field = cls.getDeclaredField("mResource")
                        break
                    } catch (_: NoSuchFieldException) {
                    }
                    cls = cls.superclass
                }
                if (field != null) {
                    field.isAccessible = true
                    val resId = field.getInt(view)
                    if (resId != 0 && resId != -1) view.resources.getResourceEntryName(resId) else null
                } else {
                    null
                }
            } catch (_: Exception) {
                null
            }

        val name = resName ?: elementId

        val imgType =
            when (drawable) {
                is VectorDrawable -> "vector"
                is BitmapDrawable -> if (resName != null) "raster" else "loaded"
                else -> "raster"
            }

        val dir = imageOutputDir ?: return Triple(name, imgType, "images/$name.png")
        val fileName = "$name.png"
        val file = java.io.File(dir, fileName)

        try {
            val origBounds = drawable.copyBounds()
            val w = drawable.intrinsicWidth.coerceAtLeast(1)
            val h = drawable.intrinsicHeight.coerceAtLeast(1)
            val maxDim = 256
            val scale = if (w > maxDim || h > maxDim) maxDim.toFloat() / maxOf(w, h) else 1f
            val bw = (w * scale).toInt().coerceAtLeast(1)
            val bh = (h * scale).toInt().coerceAtLeast(1)
            val bitmap = Bitmap.createBitmap(bw, bh, Bitmap.Config.ARGB_8888)
            val canvas = Canvas(bitmap)
            drawable.setBounds(0, 0, bw, bh)
            drawable.draw(canvas)
            drawable.bounds = origBounds
            file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 90, it) }
            bitmap.recycle()
        } catch (_: Exception) {
            return Triple(name, imgType, "images/$fileName")
        }

        return Triple(name, imgType, "images/$fileName")
    }

    private fun extractCornerRadius(view: View): Float? {
        val bg = view.background
        if (bg is GradientDrawable) {
            return try {
                val field = GradientDrawable::class.java.getDeclaredField("mRadius")
                field.isAccessible = true
                field.getFloat(bg).takeIf { it > 0f }
            } catch (_: Exception) {
                null
            }
        }
        return null
    }

    private fun extractBorder(
        view: View,
        density: Float,
    ): BorderInfo? {
        val bg = view.background
        if (bg is GradientDrawable) {
            return try {
                val widthField = GradientDrawable::class.java.getDeclaredField("mStrokeWidth")
                widthField.isAccessible = true
                val strokeWidth = widthField.getInt(bg)
                if (strokeWidth <= 0) return null
                val colorField = GradientDrawable::class.java.getDeclaredField("mStrokeColors")
                colorField.isAccessible = true
                val strokeColor = (colorField.get(bg) as? android.content.res.ColorStateList)?.defaultColor?.let { colorToHex(it) }
                BorderInfo(width = strokeWidth / density, color = strokeColor)
            } catch (_: Exception) {
                null
            }
        }
        return null
    }

    private fun extractGradient(view: View): GradientInfo? {
        val bg = view.background
        val drawable =
            when (bg) {
                is GradientDrawable -> bg
                is RippleDrawable -> if (bg.numberOfLayers > 0) bg.getDrawable(0) as? GradientDrawable else null
                else -> null
            } ?: return null

        if (Build.VERSION.SDK_INT < 24) return null
        val colors = drawable.colors ?: return null
        if (colors.size < 2) return null

        val type =
            when (drawable.gradientType) {
                GradientDrawable.LINEAR_GRADIENT -> "linear"
                GradientDrawable.RADIAL_GRADIENT -> "radial"
                GradientDrawable.SWEEP_GRADIENT -> "sweep"
                else -> "linear"
            }

        val orientation =
            if (drawable.gradientType == GradientDrawable.LINEAR_GRADIENT) {
                drawable.orientation?.name?.lowercase()
            } else {
                null
            }

        return GradientInfo(
            type = type,
            colors = colors.map { colorToHex(it) },
            orientation = orientation,
        )
    }

    private val externalRenderPatterns =
        listOf(
            "MapView",
            "MapFragment",
            "SurfaceView",
            "TextureView",
            "VideoView",
            "WebView",
            "GLSurfaceView",
            "ExoPlayerView",
        )

    private fun isExternalSurface(view: View): Boolean {
        var cls: Class<*>? = view.javaClass
        while (cls != null && cls != View::class.java) {
            val name = cls.simpleName
            if (externalRenderPatterns.any { name.contains(it) }) return true
            cls = cls.superclass
        }
        return false
    }

    private fun hasClickableAncestor(view: View): Boolean {
        var parent = view.parent
        while (parent is View) {
            if ((parent as View).isClickable || (parent as View).isLongClickable) return true
            parent = parent.getParent()
        }
        return false
    }

    private fun findClickableAncestorBounds(view: View): Bounds? {
        var parent = view.parent
        while (parent is View) {
            val p = parent as View
            if (p.isClickable || p.isLongClickable) {
                val rect = android.graphics.Rect()
                if (p.getGlobalVisibleRect(rect)) {
                    return Bounds(rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top)
                }
            }
            parent = p.parent
        }
        return null
    }

    private fun extractResourceId(view: View): String? {
        if (view.id == View.NO_ID) return null
        return try {
            view.resources.getResourceEntryName(view.id)
        } catch (_: Exception) {
            null
        }
    }

    private fun colorToHex(color: Int): String {
        val a = (color shr 24) and 0xFF
        val r = (color shr 16) and 0xFF
        val g = (color shr 8) and 0xFF
        val b = color and 0xFF
        return if (a == 255) {
            String.format("#%02X%02X%02X", r, g, b)
        } else {
            String.format("#%02X%02X%02X%02X", a, r, g, b)
        }
    }

    private fun boundsContains(
        outer: Bounds,
        inner: Bounds,
        tolerance: Int = 2,
    ): Boolean =
        inner.x >= outer.x - tolerance && inner.y >= outer.y - tolerance &&
            inner.x + inner.w <= outer.x + outer.w + tolerance &&
            inner.y + inner.h <= outer.y + outer.h + tolerance

    private fun slugify(s: String): String =
        s
            .map { c -> if (c.isLetterOrDigit()) c.lowercaseChar() else '_' }
            .joinToString("")
            .split("_")
            .filter { it.isNotEmpty() }
            .joinToString("_")
}
