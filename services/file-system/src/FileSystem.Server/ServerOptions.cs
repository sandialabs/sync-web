using System.ComponentModel.DataAnnotations;

namespace FileSystem.Server;

public sealed class ServerOptions
{
    [Required]
    public string Mode { get; set; } = "bootstrap";

    public bool ExitAfterStartup { get; set; } = false;

    [Range(1, 65535)]
    public int Port { get; set; } = 445;

    [Required]
    public string ShareName { get; set; } = "sync";

    [Required]
    public string StaticRoot { get; set; } = "/srv/share";

    [Required]
    public string Backend { get; set; } = "disk";

    [Required]
    public string JsonFixturePath { get; set; } = "/workspace/tests/static-tree.json";

    public bool EnableSmb1 { get; set; } = false;

    public bool EnableSmb2 { get; set; } = true;

    public bool EnableSmb3 { get; set; } = false;

    public bool AllowGuest { get; set; } = true;

    public string GuestAccountName { get; set; } = "Guest";

    public string? UserName { get; set; }

    public string? Password { get; set; }

    // GatewayBaseUrl and GatewayTimeoutMs are used by the http-gateway-* backends and are retained
    // for the planned Kratos session token path (see HttpGatewayClient.cs).
    // GatewayAuthToken is intentionally absent: the bearer token auth path has been removed from
    // the gateway; per-user auth will use X-Session-Token obtained via a Kratos API flow instead.
    [Required]
    public string GatewayBaseUrl { get; set; } = "http://gateway/api/v1";

    [Required]
    public string JournalJsonUrl { get; set; } = "http://journal/interface/json";

    [Range(1, 60000)]
    public int GatewayTimeoutMs { get; set; } = 30000;
}
