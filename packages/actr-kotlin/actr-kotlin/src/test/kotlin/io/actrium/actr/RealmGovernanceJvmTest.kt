package io.actrium.actr

import java.io.File
import java.time.Instant
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotEquals
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assume.assumeTrue

/** Runs the generated Kotlin bindings against the production Rust Issuer. */
class RealmGovernanceJvmTest {
    private class Signer : RealmClientSigner {
        private val seed = realmKeyGenerateSeed()

        override fun publicKey(): ByteArray = realmKeyPublic(seed)

        override fun sign(payload: ByteArray): ByteArray = realmKeySign(seed, payload)
    }

    private suspend fun refused(code: String, call: suspend () -> String) {
        val error = assertFailsWith<RealmMembershipException.Rejected> { call() }
        assertEquals(code, error.code)
        val status = when (code) {
            "owner_must_transfer", "owner_last_client" -> 409
            "invite_revoked" -> 410
            else -> 403
        }
        assertEquals(status.toUShort(), error.status)
    }

    @Test
    fun realIssuerGovernanceAndRetryBoundaries() {
        val fixturePath = System.getenv("ACTR_GOVERNANCE_FIXTURE")
        assumeTrue("Run tests/e2e/actr/member-governance/run.sh kotlin", fixturePath != null)
        val library = requireNotNull(System.getenv("ACTR_GOVERNANCE_LIBRARY"))
        System.setProperty("uniffi.component.actr.libraryOverride", library)
        val fixture = JSONObject(File(requireNotNull(fixturePath)).readText())
        fun member() =
            RealmMembership(
                fixture.getString("realm"), fixture.getString("provider_id"),
                fixture.getString("issuer"), Signer(), true,
            )
        runBlocking {
            val expires = Instant.now().plusSeconds(3600).toString()
            val owner = member()
            val ownerId = JSONObject(owner.join(fixture.getString("owner_invite"), "Kotlin owner")).getString("member_id")
            refused("owner_must_transfer") { owner.leave() }
            refused("owner_last_client") { owner.revokeClient(owner.keyId()) }
            val editorInvite = JSONObject(owner.inviteMember(RealmMemberRole.EDITOR, expires))
            val viewerInvite = JSONObject(owner.inviteMember(RealmMemberRole.VIEWER, expires))
            val editor = member()
            val viewer = member()
            val editorId = JSONObject(editor.join(editorInvite.getString("invite"), "Kotlin editor")).getString("member_id")
            val viewerId = JSONObject(viewer.join(viewerInvite.getString("invite"), "Kotlin viewer")).getString("member_id")
            for (client in listOf(editor, viewer)) {
                refused("forbidden") { client.inviteMember(RealmMemberRole.EDITOR, expires) }
                refused("forbidden") { client.removeMember(editorId) }
                refused("forbidden") { client.setRole(editorId, RealmMemberRole.VIEWER) }
                refused("forbidden") { client.transferOwner(editorId, RealmMemberRole.VIEWER) }
            }
            repeat(2) { assertEquals("viewer", JSONObject(owner.setRole(editorId, RealmMemberRole.VIEWER)).getString("role")) }
            val another = JSONObject(owner.inviteMember(RealmMemberRole.VIEWER, expires))
            assertNotEquals(viewerInvite.getString("invite_id"), another.getString("invite_id"))
            owner.revokeInvite(another.getString("invite_id"))
            val addDevice = JSONObject(editor.inviteClient(editorId, expires))
            val second = member()
            second.join(addDevice.getString("invite"), "Kotlin second device")
            val unused = JSONObject(editor.inviteClient(editorId, expires))
            repeat(2) { owner.removeMember(editorId) }
            refused("client_revoked") { editor.members() }
            refused("client_revoked") { second.members() }
            refused("invite_revoked") { member().join(unused.getString("invite"), "late device") }
            val transfer = JSONObject(owner.transferOwner(viewerId, RealmMemberRole.VIEWER))
            assertEquals(ownerId, transfer.getString("previous_owner"))
            assertEquals(viewerId, transfer.getString("owner"))
            refused("forbidden") { owner.transferOwner(viewerId, RealmMemberRole.VIEWER) }
            val members = JSONObject(viewer.members()).getJSONArray("members")
            assertEquals(1, (0 until members.length()).count { members.getJSONObject(it).getString("role") == "owner" })
            owner.leave()
            refused("client_revoked") { owner.leave() }
            viewer.members()
        }
    }
}
