using System.Windows.Forms;
using Microsoft.Web.WebView2.Core;

namespace Actrium.Windows;

internal sealed class BrowserDownloadManager
{
    private readonly IWin32Window owner;
    private readonly Action<string> setStatus;
    private readonly Action<string> setLastCompletedDownload;

    public BrowserDownloadManager(
        IWin32Window owner,
        Action<string> setStatus,
        Action<string> setLastCompletedDownload)
    {
        this.owner = owner;
        this.setStatus = setStatus;
        this.setLastCompletedDownload = setLastCompletedDownload;
    }

    public void HandleDownloadStarting(CoreWebView2DownloadStartingEventArgs args)
    {
        var operation = args.DownloadOperation;
        var suggestedFileName = BrowserDownloadPolicy.GetSuggestedFileName(
            args.ResultFilePath,
            operation.Uri);

        using var saveDialog = new SaveFileDialog
        {
            AddExtension = true,
            CheckPathExists = true,
            FileName = suggestedFileName,
            InitialDirectory = BrowserDownloadPolicy.GetDefaultDownloadsDirectory(),
            OverwritePrompt = true,
            Title = "Save Download",
            ValidateNames = true,
        };

        if (saveDialog.ShowDialog(owner) != DialogResult.OK)
        {
            args.Cancel = true;
            setStatus("Download canceled");
            return;
        }

        args.Handled = true;
        args.ResultFilePath = saveDialog.FileName;
        TrackDownload(operation, saveDialog.FileName);
        setStatus($"Downloading {Path.GetFileName(saveDialog.FileName)}");
    }

    private void TrackDownload(CoreWebView2DownloadOperation operation, string filePath)
    {
        var fileName = Path.GetFileName(filePath);
        EventHandler<object>? bytesReceivedChanged = null;
        EventHandler<object>? stateChanged = null;

        bytesReceivedChanged = (_, _) =>
        {
            if (operation.State == CoreWebView2DownloadState.InProgress)
            {
                setStatus($"Downloading {fileName}: {BrowserDownloadPolicy.FormatProgress(
                    NormalizeByteCount(operation.BytesReceived),
                    NormalizeOptionalByteCount(operation.TotalBytesToReceive))}");
            }
        };

        stateChanged = (_, _) =>
        {
            switch (operation.State)
            {
                case CoreWebView2DownloadState.InProgress:
                    break;
                case CoreWebView2DownloadState.Completed:
                    setLastCompletedDownload(filePath);
                    setStatus($"Downloaded {fileName}");
                    Detach();
                    break;
                case CoreWebView2DownloadState.Interrupted:
                    setStatus($"Download interrupted: {fileName}");
                    Detach();
                    break;
            }
        };

        operation.BytesReceivedChanged += bytesReceivedChanged;
        operation.StateChanged += stateChanged;

        void Detach()
        {
            operation.BytesReceivedChanged -= bytesReceivedChanged;
            operation.StateChanged -= stateChanged;
        }
    }

    private static long NormalizeByteCount(long bytes)
    {
        return Math.Max(0, bytes);
    }

    private static long NormalizeOptionalByteCount(ulong? bytes)
    {
        return bytes is ulong value ? NormalizeUnsignedByteCount(value) : -1;
    }

    private static long NormalizeUnsignedByteCount(ulong bytes)
    {
        return bytes > long.MaxValue ? long.MaxValue : (long)bytes;
    }
}
