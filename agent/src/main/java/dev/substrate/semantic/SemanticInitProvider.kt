package dev.substrate.semantic

import android.app.Application
import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.net.Uri

class SemanticInitProvider : ContentProvider() {
    override fun onCreate(): Boolean {
        val app = context?.applicationContext as? Application ?: return false
        try {
            SemanticServer.install(app)
        } catch (e: Throwable) {
            android.util.Log.e("SemanticAgent", "Failed to start semantic server: ${e.message}", e)
        }
        return true
    }

    override fun query(
        uri: Uri,
        p: Array<String>?,
        s: String?,
        sa: Array<String>?,
        so: String?,
    ): Cursor? = null

    override fun getType(uri: Uri): String? = null

    override fun insert(
        uri: Uri,
        values: ContentValues?,
    ): Uri? = null

    override fun delete(
        uri: Uri,
        s: String?,
        sa: Array<String>?,
    ): Int = 0

    override fun update(
        uri: Uri,
        v: ContentValues?,
        s: String?,
        sa: Array<String>?,
    ): Int = 0
}
