package dev.substrate.semantic

import android.app.Activity
import android.content.Intent

interface AgentNavigator {
    fun createSiteIntent(activity: Activity, siteId: Int): Intent
    fun createUserIntent(activity: Activity, userId: Int): Intent
}

interface AgentAuth {
    suspend fun login(email: String, password: String): Result<Unit>
    suspend fun logout()
    suspend fun isAuthenticated(): Boolean
    suspend fun getUserId(): Int
    suspend fun resetState()
    suspend fun deleteQuestion(questionId: Int): Boolean
}
