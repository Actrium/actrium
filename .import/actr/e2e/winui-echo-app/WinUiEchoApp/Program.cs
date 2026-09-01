using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace WinUiEchoApp;

public static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        RuntimeLog.Info("Program.Main entered");
        try
        {
            WinRT.ComWrappersSupport.InitializeComWrappers();
            RuntimeLog.Info("WinRT COM wrappers initialized");

            Application.Start(p =>
            {
                RuntimeLog.Info("Application.Start callback entered");
                try
                {
                    var dispatcherQueue = DispatcherQueue.GetForCurrentThread();
                    RuntimeLog.Info($"DispatcherQueue available={dispatcherQueue is not null}");
                    if (dispatcherQueue is null)
                    {
                        throw new InvalidOperationException("WinUI did not create a DispatcherQueue for the startup thread.");
                    }

                    SynchronizationContext.SetSynchronizationContext(
                        new DispatcherQueueSynchronizationContext(dispatcherQueue));
                    RuntimeLog.Info("Dispatcher synchronization context configured");

                    _ = new App();
                    RuntimeLog.Info("App instance created");
                }
                catch (Exception ex)
                {
                    RuntimeLog.Error("Application.Start callback failed", ex);
                    throw;
                }
            });

            RuntimeLog.Info("Application.Start returned");
        }
        catch (Exception ex)
        {
            RuntimeLog.Error("Program.Main failed", ex);
            throw;
        }
    }
}
