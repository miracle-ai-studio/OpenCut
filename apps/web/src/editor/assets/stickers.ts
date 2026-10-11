import triangle from "./catalog/triangle.svg?raw";
import hexagon from "./catalog/hexagon.svg?raw";
import diamond from "./catalog/diamond.svg?raw";
import star from "./catalog/star.svg?raw";
export const stickers = {
  Triangle: triangle,
  Hexagon: hexagon,
  Diamond: diamond,
  Star: star,
};
export async function stickerFile(
  name: keyof typeof stickers,
  color: string,
): Promise<File> {
  if (!/^#[0-9a-f]{6}$/i.test(color)) throw new Error("Invalid sticker color");
  const svg = stickers[name].replaceAll("currentColor", color);
  const image = new Image();
  const url = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
  try {
    image.src = url;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = 1000;
    canvas.height = 1000;
    canvas.getContext("2d")!.drawImage(image, 0, 0, 1000, 1000);
    const blob = await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob(
        (blob) =>
          blob ? resolve(blob) : reject(new Error("Cannot create sticker")),
        "image/png",
      ),
    );
    return new File([blob], `${name}.png`, { type: "image/png" });
  } finally {
    URL.revokeObjectURL(url);
  }
}
