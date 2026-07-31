import type { ExportRequest, ExportResult } from "./types";

const PNG_PREFIX = "data:image/png;base64,";
const JPEG_PREFIX = "data:image/jpeg;base64,";
const PNG_SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10] as const;
const PDF_PAGE_WIDTH = 841.89;
const PDF_PAGE_HEIGHT = 595.28;
const PDF_MARGIN = 18;

function decodeBase64DataUrl(dataUrl: string, prefix: string): Uint8Array {
  if (!dataUrl.startsWith(prefix)) {
    throw new Error("导出报告包含不支持的图像格式。");
  }
  let binary: string;
  try {
    binary = atob(dataUrl.slice(prefix.length));
  } catch {
    throw new Error("导出报告包含无效的 Base64 图像。");
  }
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function reportPngBytes(dataUrl: string): Uint8Array {
  const bytes = decodeBase64DataUrl(dataUrl, PNG_PREFIX);
  if (
    bytes.length < PNG_SIGNATURE.length ||
    PNG_SIGNATURE.some((expected, index) => bytes[index] !== expected)
  ) {
    throw new Error("导出报告不是有效的 PNG 图像。");
  }
  return bytes;
}

function ascii(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

function concatBytes(parts: readonly Uint8Array[]): Uint8Array {
  const length = parts.reduce((total, part) => total + part.length, 0);
  const result = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

export function buildPdfFromJpeg(
  jpegBytes: Uint8Array,
  imageWidth: number,
  imageHeight: number,
): Uint8Array {
  if (jpegBytes.length < 4 || jpegBytes[0] !== 0xff || jpegBytes[1] !== 0xd8) {
    throw new Error("PDF 导出没有得到有效的 JPEG 报告画布。");
  }
  if (
    !Number.isSafeInteger(imageWidth) ||
    !Number.isSafeInteger(imageHeight) ||
    imageWidth <= 0 ||
    imageHeight <= 0
  ) {
    throw new Error("PDF 导出图像尺寸无效。");
  }

  const scale = Math.min(
    (PDF_PAGE_WIDTH - PDF_MARGIN * 2) / imageWidth,
    (PDF_PAGE_HEIGHT - PDF_MARGIN * 2) / imageHeight,
  );
  const drawWidth = imageWidth * scale;
  const drawHeight = imageHeight * scale;
  const drawX = (PDF_PAGE_WIDTH - drawWidth) / 2;
  const drawY = (PDF_PAGE_HEIGHT - drawHeight) / 2;
  const content = ascii(
    `q\n${drawWidth.toFixed(3)} 0 0 ${drawHeight.toFixed(3)} ${drawX.toFixed(3)} ${drawY.toFixed(3)} cm\n/Im0 Do\nQ\n`,
  );

  const parts: Uint8Array[] = [];
  const offsets = Array<number>(6).fill(0);
  let length = 0;
  const push = (part: Uint8Array) => {
    parts.push(part);
    length += part.length;
  };
  const pushObject = (id: number, objectParts: readonly Uint8Array[]) => {
    offsets[id] = length;
    push(ascii(`${id} 0 obj\n`));
    objectParts.forEach(push);
    push(ascii("\nendobj\n"));
  };

  push(
    new Uint8Array([
      0x25, 0x50, 0x44, 0x46, 0x2d, 0x31, 0x2e, 0x34, 0x0a,
      0x25, 0xe2, 0xe3, 0xcf, 0xd3, 0x0a,
    ]),
  );
  pushObject(1, [ascii("<< /Type /Catalog /Pages 2 0 R >>")]);
  pushObject(2, [ascii("<< /Type /Pages /Kids [3 0 R] /Count 1 >>")]);
  pushObject(3, [
    ascii(
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${PDF_PAGE_WIDTH} ${PDF_PAGE_HEIGHT}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>`,
    ),
  ]);
  pushObject(4, [
    ascii(
      `<< /Type /XObject /Subtype /Image /Width ${imageWidth} /Height ${imageHeight} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${jpegBytes.length} >>\nstream\n`,
    ),
    jpegBytes,
    ascii("\nendstream"),
  ]);
  pushObject(5, [
    ascii(`<< /Length ${content.length} >>\nstream\n`),
    content,
    ascii("endstream"),
  ]);

  const xrefOffset = length;
  const xrefRows = offsets
    .slice(1)
    .map((offset) => `${String(offset).padStart(10, "0")} 00000 n \n`)
    .join("");
  push(
    ascii(
      `xref\n0 6\n0000000000 65535 f \n${xrefRows}trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xrefOffset}\n%%EOF\n`,
    ),
  );
  return concatBytes(parts);
}

function loadImage(dataUrl: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("无法读取报告画布，PDF 导出已停止。"));
    image.src = dataUrl;
  });
}

async function reportPdfBytes(reportPngDataUrl: string): Promise<Uint8Array> {
  reportPngBytes(reportPngDataUrl);
  const image = await loadImage(reportPngDataUrl);
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth || image.width;
  canvas.height = image.naturalHeight || image.height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("当前浏览器无法创建 PDF 导出画布。");
  context.fillStyle = "#ffffff";
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.drawImage(image, 0, 0, canvas.width, canvas.height);
  const jpegBytes = decodeBase64DataUrl(
    canvas.toDataURL("image/jpeg", 0.94),
    JPEG_PREFIX,
  );
  return buildPdfFromJpeg(jpegBytes, canvas.width, canvas.height);
}

function downloadBytes(bytes: Uint8Array, mime: string, fileName: string): void {
  const buffer = bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  ) as ArrayBuffer;
  const objectUrl = URL.createObjectURL(new Blob([buffer], { type: mime }));
  const anchor = document.createElement("a");
  anchor.href = objectUrl;
  anchor.download = fileName;
  anchor.style.display = "none";
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  window.setTimeout(() => URL.revokeObjectURL(objectUrl), 0);
}

export async function exportReportInBrowser(
  request: ExportRequest,
): Promise<ExportResult> {
  if (!/^[A-Za-z0-9._-]+$/.test(request.suggestedFileName)) {
    throw new Error("导出文件名包含不安全字符。");
  }
  const expectedExtension = `.${request.format}`;
  if (!request.suggestedFileName.toLowerCase().endsWith(expectedExtension)) {
    throw new Error(`导出文件名必须以 ${expectedExtension} 结尾。`);
  }

  const bytes =
    request.format === "png"
      ? reportPngBytes(request.reportPngDataUrl)
      : await reportPdfBytes(request.reportPngDataUrl);
  const mime = request.format === "png" ? "image/png" : "application/pdf";
  downloadBytes(bytes, mime, request.suggestedFileName);
  return {
    cancelled: false,
    path: null,
    bytesWritten: bytes.byteLength,
  };
}
