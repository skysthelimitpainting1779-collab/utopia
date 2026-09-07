import { issueSignedToken, presignUrl } from "@vercel/blob";

export const MAX_BLOB_BYTES = 100 * 1024 * 1024;
const DELEGATION_TTL_MS = 5 * 60 * 1000;
const URL_TTL_MS = 2 * 60 * 1000;
const CONTENT_PATH = /^files\/[0-9a-f]{64}$/;
const OPERATIONS = ["get", "head", "put", "delete"] as const;

export type BlobOperation = (typeof OPERATIONS)[number];

export type PresignRequest = {
  pathname: string;
  operation: BlobOperation;
};

function isRecord(input: unknown): input is Record<string, unknown> {
  return typeof input === "object" && input !== null && !Array.isArray(input);
}

function isOperation(input: unknown): input is BlobOperation {
  return typeof input === "string" && OPERATIONS.includes(input as BlobOperation);
}

export function parsePresignRequest(input: unknown): PresignRequest {
  if (!isRecord(input)) throw new Error("Presign request must be an object");
  if (typeof input.pathname !== "string" || !CONTENT_PATH.test(input.pathname)) {
    throw new Error("pathname must match files/{lowercase sha256}");
  }
  if (!isOperation(input.operation)) {
    throw new Error("operation must be get, head, put, or delete");
  }
  return { pathname: input.pathname, operation: input.operation };
}

export async function createPresignedBlobUrl(
  request: PresignRequest,
  now = Date.now(),
): Promise<string> {
  const delegationUntil = now + DELEGATION_TTL_MS;
  const urlUntil = now + URL_TTL_MS;
  const token = await issueSignedToken({
    pathname: request.pathname,
    operations: [request.operation],
    validUntil: delegationUntil,
    ...(request.operation === "put"
      ? { maximumSizeInBytes: MAX_BLOB_BYTES }
      : {}),
  });

  switch (request.operation) {
    case "get":
      return (
        await presignUrl(token, {
          operation: "get",
          pathname: request.pathname,
          access: "private",
          validUntil: urlUntil,
        })
      ).presignedUrl;
    case "head":
      return (
        await presignUrl(token, {
          operation: "head",
          pathname: request.pathname,
          access: "private",
          validUntil: urlUntil,
        })
      ).presignedUrl;
    case "put":
      return (
        await presignUrl(token, {
          operation: "put",
          pathname: request.pathname,
          access: "private",
          validUntil: urlUntil,
          maximumSizeInBytes: MAX_BLOB_BYTES,
          addRandomSuffix: false,
          allowOverwrite: false,
        })
      ).presignedUrl;
    case "delete":
      return (
        await presignUrl(token, {
          operation: "delete",
          pathname: request.pathname,
          access: "private",
          validUntil: urlUntil,
        })
      ).presignedUrl;
  }
}
