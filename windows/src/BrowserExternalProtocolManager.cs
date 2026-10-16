using System.Diagnostics;
using System.Windows.Forms;

namespace Actrium.Windows;

internal sealed class BrowserExternalProtocolManager
{
    private readonly IWin32Window owner;
    private readonly Action<string> setStatus;

    public BrowserExternalProtocolManager(IWin32Window owner, Action<string> setStatus)
    {
        this.owner = owner;
        this.setStatus = setStatus;
    }

    public bool HandleExternalProtocol(string address)
    {
        if (!BrowserExternalProtocolPolicy.TryCreateExternalProtocolPrompt(address, out var prompt))
        {
            return false;
        }

        var result = MessageBox.Show(
            owner,
            prompt.Message,
            prompt.Title,
            MessageBoxButtons.YesNo,
            MessageBoxIcon.Question,
            MessageBoxDefaultButton.Button2);

        if (result != DialogResult.Yes)
        {
            setStatus($"Blocked {prompt.Scheme} link");
            return true;
        }

        try
        {
            using var process = Process.Start(new ProcessStartInfo
            {
                FileName = prompt.Address,
                UseShellExecute = true,
            });
            setStatus($"Opened {prompt.Scheme} link");
        }
        catch (Exception error) when (error is InvalidOperationException or System.ComponentModel.Win32Exception)
        {
            setStatus($"Could not open {prompt.Scheme} link");
        }

        return true;
    }
}
