using System.Windows.Forms;
using Microsoft.Web.WebView2.Core;

namespace Actro.Windows;

internal sealed class BrowserPermissionManager
{
    private readonly IWin32Window owner;
    private readonly Action<string> setStatus;

    public BrowserPermissionManager(IWin32Window owner, Action<string> setStatus)
    {
        this.owner = owner;
        this.setStatus = setStatus;
    }

    public void HandlePermissionRequested(CoreWebView2PermissionRequestedEventArgs args)
    {
        args.Handled = true;
        var prompt = BrowserPermissionPolicy.BuildPrompt(args.PermissionKind.ToString(), args.Uri);
        var result = MessageBox.Show(
            owner,
            prompt.Message,
            prompt.Title,
            MessageBoxButtons.YesNo,
            MessageBoxIcon.Question,
            MessageBoxDefaultButton.Button2);

        if (result == DialogResult.Yes)
        {
            args.State = CoreWebView2PermissionState.Allow;
            setStatus($"Allowed {prompt.PermissionLabel} for {prompt.Origin}");
            return;
        }

        args.State = CoreWebView2PermissionState.Deny;
        setStatus($"Blocked {prompt.PermissionLabel} for {prompt.Origin}");
    }
}
