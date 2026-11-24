package app.nocturne.system

import android.accounts.Account
import android.accounts.AccountManager
import android.content.Context
import com.google.android.gms.auth.api.identity.AuthorizationClient
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.AuthorizationResult
import com.google.android.gms.auth.api.identity.ClearTokenRequest
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.CommonStatusCodes
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Task
import com.google.android.gms.tasks.Tasks
import java.io.File
import java.util.concurrent.ExecutionException
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException

/**
 * The Google account the backup uses, and its permission for files the app makes in the Drive.
 * The phone's account picker and Google's own screen ask once ([SystemPlugin.placeConnect]);
 * after that a token comes without a screen, also in the background, until the person takes
 * the permission back or the account leaves the phone. The app holds no client ID: Google knows
 * it by its package and signature.
 */
class DriveAccount(private val context: Context) : Tokens {
    companion object {
        const val SCOPE = "https://www.googleapis.com/auth/drive.file"
        const val GOOGLE = "com.google"
        private const val WAIT_SECONDS = 60L

        /** What a failed call to Google's services means for the backup. */
        fun outcome(status: Int): PlaceOutcome = when (status) {
            CommonStatusCodes.NETWORK_ERROR, CommonStatusCodes.TIMEOUT -> PlaceOutcome.Offline
            // Whether the account left the phone is checked before asking; here it is there.
            CommonStatusCodes.SIGN_IN_REQUIRED, CommonStatusCodes.RESOLUTION_REQUIRED ->
                PlaceOutcome.NoPermission
            else -> PlaceOutcome.Failed
        }
    }

    private val client: AuthorizationClient = Identity.getAuthorizationClient(context)
    private val file = File(context.noBackupFilesDir, "backup-account")

    /** The account's address, once the person chose one. */
    fun chosen(): String? = try {
        file.takeIf { it.isFile }?.readText()?.trim()?.ifEmpty { null }
    } catch (e: java.io.IOException) {
        null
    }

    fun keep(email: String) {
        file.parentFile?.mkdirs()
        val staged = File(file.parentFile, "backup-account.new")
        staged.writeText(email)
        if (!staged.renameTo(file)) {
            staged.delete()
            throw java.io.IOException("not kept")
        }
    }

    /** The request for the Drive permission for the account `email`. */
    fun request(email: String): AuthorizationRequest = AuthorizationRequest.Builder()
        .setRequestedScopes(listOf(Scope(SCOPE)))
        .setAccount(Account(email, GOOGLE))
        .build()

    fun authorize(email: String): Task<AuthorizationResult> = client.authorize(request(email))

    fun resultFrom(data: android.content.Intent?): AuthorizationResult =
        client.getAuthorizationResultFromIntent(data)

    private fun <T> await(task: Task<T>): T = try {
        Tasks.await(task, WAIT_SECONDS, TimeUnit.SECONDS)
    } catch (e: ExecutionException) {
        val cause = e.cause
        val status = (cause as? ApiException)?.statusCode ?: CommonStatusCodes.ERROR
        throw PlaceFailure(outcome(status), cause?.message ?: "authorization")
    } catch (e: TimeoutException) {
        throw PlaceFailure(PlaceOutcome.Offline, "authorization timed out")
    }

    /**
     * Whether the account `email` is still on the phone. The account picker made it visible to
     * the app, so it is listed for as long as it stays.
     */
    fun onPhone(email: String): Boolean =
        AccountManager.get(context).getAccountsByType(GOOGLE)
            .any { it.name.equals(email, ignoreCase = true) }

    /**
     * A token for the chosen account without a screen. Signed out when the account left the
     * phone; no permission when Google's screen would be needed again.
     */
    override fun token(): String {
        val email = chosen() ?: throw PlaceFailure(PlaceOutcome.SignedOut, "no account chosen")
        if (!onPhone(email)) throw PlaceFailure(PlaceOutcome.SignedOut, "account not on phone")
        val result = await(authorize(email))
        if (result.hasResolution()) {
            throw PlaceFailure(PlaceOutcome.NoPermission, "permission needed")
        }
        return result.accessToken ?: throw PlaceFailure(PlaceOutcome.NoPermission, "no token")
    }

    override fun renew(stale: String): String {
        await(client.clearToken(ClearTokenRequest.builder().setToken(stale).build()))
        return token()
    }
}
