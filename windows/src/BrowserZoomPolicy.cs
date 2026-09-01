namespace Actro.Windows;

internal static class BrowserZoomPolicy
{
    public const double DefaultZoomFactor = 1.0;
    public const double MinimumZoomFactor = 0.25;
    public const double MaximumZoomFactor = 5.0;
    public const double ZoomStep = 0.1;

    public static double ZoomIn(double zoomFactor)
    {
        return Normalize(zoomFactor + ZoomStep);
    }

    public static double ZoomOut(double zoomFactor)
    {
        return Normalize(zoomFactor - ZoomStep);
    }

    public static double Reset()
    {
        return DefaultZoomFactor;
    }

    public static bool CanZoomIn(double zoomFactor)
    {
        return Normalize(zoomFactor) < MaximumZoomFactor;
    }

    public static bool CanZoomOut(double zoomFactor)
    {
        return Normalize(zoomFactor) > MinimumZoomFactor;
    }

    public static bool CanReset(double zoomFactor)
    {
        return Math.Abs(Normalize(zoomFactor) - DefaultZoomFactor) > 0.001;
    }

    public static string FormatZoom(double zoomFactor)
    {
        return $"{Normalize(zoomFactor) * 100:0}%";
    }

    public static double Normalize(double zoomFactor)
    {
        if (double.IsNaN(zoomFactor) || double.IsInfinity(zoomFactor))
        {
            return DefaultZoomFactor;
        }

        var rounded = Math.Round(zoomFactor, 2, MidpointRounding.AwayFromZero);
        return Math.Clamp(rounded, MinimumZoomFactor, MaximumZoomFactor);
    }
}
