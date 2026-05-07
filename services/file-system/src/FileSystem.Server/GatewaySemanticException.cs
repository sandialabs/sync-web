using System.Net;
using System.Text.Json.Nodes;

namespace FileSystem.Server;

public sealed class GatewaySemanticException : Exception
{
    public string Code { get; }
    public JsonNode? Body { get; }
    public HttpStatusCode StatusCode { get; }

    public GatewaySemanticException(string code, string message, JsonNode? body, HttpStatusCode statusCode)
        : base(message)
    {
        Code = code;
        Body = body;
        StatusCode = statusCode;
    }
}
