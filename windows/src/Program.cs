using System.Diagnostics;
using System.Windows.Forms;

namespace Actrium.Windows;

internal static class Program
{
    [STAThread]
    private static void Main(string[] args)
    {
        ApplicationConfiguration.Initialize();

        // Fail fast when the Rust core library cannot be loaded: every
        // durable contract (schema, SQL, normalization, sync policy) lives
        // there, so running without it would silently corrupt behavior.
        var coreStatus = ActriumCore.LoadStatus();
        if (!coreStatus.IsAvailable)
        {
            var reason = string.IsNullOrWhiteSpace(coreStatus.UnavailableReason)
                ? "The Rust core library (actrium_core.dll) could not be loaded."
                : coreStatus.UnavailableReason;
            Trace.TraceError("Rust core library unavailable at startup: {0}", reason);
            if (Environment.UserInteractive)
            {
                MessageBox.Show(
                    reason,
                    "Actrium cannot start",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Error);
            }
            else
            {
                // Non-interactive hosts (services, CI, session 0) cannot show
                // modal UI: MessageBox.Show would itself throw
                // InvalidOperationException and turn a clean fail-fast into an
                // unhandled crash, so log-and-exit is the whole story here.
                Trace.TraceWarning(
                    "Skipping startup error dialog: no interactive desktop is available.");
            }
            Environment.Exit(1);
            return;
        }

        var options = WindowsStartupOptions.FromArgs(args);

        // Mirror --user-data-root into the single override choke point before
        // any durable store is constructed. WindowsAppPaths.GetLocalAppDataRoot
        // reads this variable, so both the argument and a directly exported
        // ACTRIUM_USER_DATA_ROOT share one mechanism. Arg parsing happens before
        // BrowserApplicationContext builds the stores, so the ordering is safe.
        if (!string.IsNullOrWhiteSpace(options.UserDataRoot))
        {
            Environment.SetEnvironmentVariable(
                WindowsAppPaths.UserDataRootEnvironmentVariable,
                options.UserDataRoot);
        }

        Application.Run(new BrowserApplicationContext(options));
    }
}
