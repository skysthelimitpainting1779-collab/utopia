import { createPresignedBlobUrl, parsePresignRequest } from "../../../../lib/blob-presign";
import { authorizeInternalRequest } from "../../../../lib/internal-auth";

export const runtime = "nodejs";

export async function POST(request: Request): Promise<Response> {
  const denied = authorizeInternalRequest(request);
  if (denied) return denied;

  let parsed;
  try {
    parsed = parsePresignRequest(await request.json());
  } catch (error) {
    return Response.json(
      { error: error instanceof Error ? error.message : "Invalid request" },
      { status: 422 },
    );
  }

  try {
    const presignedUrl = await createPresignedBlobUrl(parsed);
    return Response.json({ presignedUrl });
  } catch (error) {
    console.error(
      "Private Blob presign failed",
      error instanceof Error ? error.name : "UnknownError",
    );
    return Response.json({ error: "Blob presign failed" }, { status: 502 });
  }
}
