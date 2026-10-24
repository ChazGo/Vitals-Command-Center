using System.Runtime.InteropServices;
using System.Text.Json;

namespace Vitals.Windows;

internal static class CoreClient
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = true
    };

    [DllImport("vitals_core.dll", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern IntPtr vitals_call([MarshalAs(UnmanagedType.LPUTF8Str)] string request_utf8);

    [DllImport("vitals_core.dll", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern void vitals_free(IntPtr result);

    // The caller serializes operations and runs this blocking boundary off the UI thread.
    internal static HealthStatus Call(object request)
    {
        IntPtr result = IntPtr.Zero;
        try
        {
            result = vitals_call(JsonSerializer.Serialize(request));
            if (result == IntPtr.Zero)
                throw new InvalidOperationException("The native health service returned an empty response.");

            string json = Marshal.PtrToStringUTF8(result)
                ?? throw new InvalidOperationException("The native health service returned an unreadable response.");
            var response = JsonSerializer.Deserialize<CoreResponse>(json, JsonOptions)
                ?? throw new InvalidOperationException("The native health service returned an invalid response.");
            if (!response.Ok)
                throw new InvalidOperationException(response.Error ?? "The health operation could not be completed.");

            return response.Data
                ?? throw new InvalidOperationException("The native health service did not return a health status.");
        }
        finally
        {
            if (result != IntPtr.Zero)
                vitals_free(result);
        }
    }

    private sealed class CoreResponse
    {
        public CoreResponse() { }

        public bool Ok { get; set; }
        public HealthStatus? Data { get; set; }
        public string? Error { get; set; }
    }
}

internal sealed class HealthStatus
{
    public HealthStatus() { }

    public bool Configured { get; set; }
    public bool Connected { get; set; }
    public string ClientId { get; set; } = "";
    public string? LastSync { get; set; }
    public List<HealthDay> Days { get; set; } = [];
    public List<string> Warnings { get; set; } = [];
}

internal sealed class HealthDay
{
    public HealthDay() { }

    public string Date { get; set; } = "";
    public double? Steps { get; set; }
    public double? RestingHeartRate { get; set; }
    public double? Hrv { get; set; }
    public double? Spo2 { get; set; }
    public double? RespiratoryRate { get; set; }
    public double? SleepHours { get; set; }
    public double? DeepHours { get; set; }
    public double? RemHours { get; set; }
    public double? LightHours { get; set; }
    public double? ActiveCalories { get; set; }

    public bool HasData => Steps.HasValue || RestingHeartRate.HasValue || Hrv.HasValue
        || Spo2.HasValue || RespiratoryRate.HasValue || SleepHours.HasValue
        || DeepHours.HasValue || RemHours.HasValue || LightHours.HasValue || ActiveCalories.HasValue;
}
