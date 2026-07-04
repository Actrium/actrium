using System.Diagnostics;
using System.Windows.Forms;

namespace Actro.Windows;

internal static class Program
{
    [STAThread]
    private static void Main(string[] args)
    {
        ApplicationConfiguration.Initialize();

        // Fail fast when the Rust core library cannot be loaded: every
        // durable contract (schema, SQL, normalization, sync policy) lives
        // there, so running without it would silently corrupt behavior.
        var coreStatus = ActroCore.LoadStatus();
        if (!coreStatus.IsAvailable)
        {
            var reason = string.IsNullOrWhiteSpace(coreStatus.UnavailableReason)
                ? "The Rust core library (actro_core.dll) could not be loaded."
                : coreStatus.UnavailableReason;
            Trace.TraceError("Rust core library unavailable at startup: {0}", reason);
            MessageBox.Show(
                reason,
                "Actro cannot start",
                MessageBoxButtons.OK,
                MessageBoxIcon.Error);
            Environment.Exit(1);
            return;
        }

        Application.Run(new BrowserApplicationContext(WindowsStartupOptions.FromArgs(args)));
    }
}
