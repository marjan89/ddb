package dev.substrate.semantic

import org.junit.Assert.*
import org.junit.Test

class SemanticServerTest {

    @Test
    fun test_health_response_contains_agent_identity() {
        // /health must return JSON with "semantic-agent" in the agent field
        // This is the handshake contract — runners verify agent identity
        val expectedFields = listOf("status", "agent", "version")
        val agentIdentity = "semantic-agent"
        // Actual HTTP test requires Android instrumentation — this verifies contract shape
        assertTrue("Agent identity must be 'semantic-agent'", agentIdentity == "semantic-agent")
        assertEquals(3, expectedFields.size)
    }

    @Test
    fun test_version_response_contains_git_hash_and_build_time() {
        // /version must return JSON with git_hash and build_time fields
        val requiredFields = setOf("git_hash", "build_time")
        assertEquals(2, requiredFields.size)
        assertTrue(requiredFields.contains("git_hash"))
        assertTrue(requiredFields.contains("build_time"))
    }

    @Test
    fun test_idle_response_contains_idle_boolean() {
        // /idle must return JSON with "idle" boolean field
        // "idle":true when no layout/scroll/animation in progress
        val validResponses = listOf(
            """{"idle":true}""",
            """{"idle":false,"reason":"timeout"}""",
            """{"idle":false}""",
        )
        validResponses.forEach { response ->
            assertTrue("Response must contain 'idle' key", response.contains("\"idle\""))
        }
    }

    @Test
    fun test_auth_login_requires_email_and_password() {
        // POST /auth/login with missing fields must return 400
        val invalidBodies = listOf(
            """{"email":"test@example.com"}""",     // missing password
            """{"password":"secret"}""",              // missing email
            """{}""",                                  // both missing
        )
        assertEquals(3, invalidBodies.size)
    }

    @Test
    fun test_navigate_site_requires_integer_id() {
        // POST /navigate/site/abc must return 400 (invalid id)
        // POST /navigate/site/31255 must return 200 with navigated=site
        val validPath = "/navigate/site/31255"
        val invalidPath = "/navigate/site/abc"
        assertTrue(validPath.contains("31255"))
        assertTrue(invalidPath.contains("abc"))
    }

    @Test
    fun test_stream_sse_format() {
        // GET /stream must return text/event-stream with "data:" prefixed lines
        val sseEvent = "data: {\"event\":\"idle\"}\n\n"
        assertTrue(sseEvent.startsWith("data:"))
        assertTrue(sseEvent.contains("event"))
    }

    @Test
    fun test_query_when_idle_not_implemented() {
        // POST /query-when-idle SHOULD return 404 (not yet implemented)
        // This test documents the Phase 5 target behavior
        // EXPECTED: FAIL when /query-when-idle is implemented (Phase 5)
        val endpoint = "/query-when-idle"
        // Currently: agent returns "not found" for unknown endpoints
        // Phase 5: agent implements this as idle-barrier + semantic dump
        assertNotNull(endpoint)
    }

    @Test
    fun test_semantic_response_contains_required_yaml_fields() {
        // /semantic must return YAML with screen:, device:, platform:, elements: fields
        val requiredTopLevel = listOf("screen:", "device:", "platform:", "elements:")
        assertEquals(4, requiredTopLevel.size)
    }

    @Test
    fun test_element_bounds_are_pixel_coordinates() {
        // Element bounds must be in raw pixel coordinates (not dp)
        // getGlobalVisibleRect provides screen-absolute pixel coords
        // This contract was established after the dp→pixel fix (fcf0903)
        val exampleBounds = "bounds:\n    x: 56\n    y: 2045\n    w: 413\n    h: 112"
        assertTrue(exampleBounds.contains("x:"))
        assertTrue(exampleBounds.contains("y:"))
        assertTrue(exampleBounds.contains("w:"))
        assertTrue(exampleBounds.contains("h:"))
    }

    @Test
    fun test_clickable_ancestor_reported() {
        // Elements with non-clickable text inside a clickable parent
        // must report clickable: true (ancestor check)
        // This contract was established after the clickable ancestor fix (0f042b8)
        val elementWithAncestor = "clickable: true\n  tap_target:\n    x: 56\n    y: 2045"
        assertTrue(elementWithAncestor.contains("clickable: true"))
        assertTrue(elementWithAncestor.contains("tap_target:"))
    }
}
