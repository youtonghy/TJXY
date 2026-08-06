import { CanvasTexture, LinearFilter, SRGBColorSpace } from 'three';

export interface ProceduralFilmTexture {
  dispose: () => void;
  texture: CanvasTexture;
  update: (elapsedMilliseconds: number, colourMix: number) => void;
}

const WIDTH = 512;
const HEIGHT = 288;
const FRAME_INTERVAL_MS = 50;

export function createProceduralFilmTexture(): ProceduralFilmTexture {
  const canvas = document.createElement('canvas');
  canvas.width = WIDTH;
  canvas.height = HEIGHT;
  const context = canvas.getContext('2d', { alpha: false });
  if (!context) throw new Error('Canvas 2D is unavailable.');

  const grainCanvas = document.createElement('canvas');
  grainCanvas.width = 128;
  grainCanvas.height = 72;
  const grainContext = grainCanvas.getContext('2d', { alpha: true });
  if (!grainContext) throw new Error('Canvas 2D is unavailable.');
  const grain = grainContext.createImageData(grainCanvas.width, grainCanvas.height);

  const texture = new CanvasTexture(canvas);
  texture.colorSpace = SRGBColorSpace;
  texture.magFilter = LinearFilter;
  texture.minFilter = LinearFilter;
  texture.generateMipmaps = false;

  let lastFrame = -1;
  const update = (elapsedMilliseconds: number, colourMix: number) => {
    const frame = Math.floor(Math.max(elapsedMilliseconds, 0) / FRAME_INTERVAL_MS);
    if (frame === lastFrame) return;
    lastFrame = frame;
    drawFilmFrame(context, grainContext, grain, frame, clamp01(colourMix));
    texture.needsUpdate = true;
  };

  update(0, 0);
  return { dispose: () => { texture.dispose(); }, texture, update };
}

function drawFilmFrame(
  context: CanvasRenderingContext2D,
  grainContext: CanvasRenderingContext2D,
  grain: ImageData,
  frame: number,
  colourMix: number,
) {
  const seconds = frame * FRAME_INTERVAL_MS / 1_000;
  const weaveX = Math.sin(frame * 1.71) * 2.4;
  const weaveY = Math.cos(frame * 1.13) * 1.5;
  const skyTop = mixRgb([194, 194, 184], [51, 139, 160], colourMix);
  const skyBottom = mixRgb([104, 104, 100], [245, 180, 100], colourMix);
  const ground = mixRgb([43, 43, 42], [30, 70, 66], colourMix);
  const silhouette = mixRgb([25, 25, 24], [38, 32, 58], colourMix);

  context.save();
  context.translate(weaveX, weaveY);
  const sky = context.createLinearGradient(0, 0, WIDTH, HEIGHT);
  sky.addColorStop(0, rgb(skyTop));
  sky.addColorStop(1, rgb(skyBottom));
  context.fillStyle = sky;
  context.fillRect(-8, -8, WIDTH + 16, HEIGHT + 16);

  context.filter = 'blur(5px)';
  const sunX = WIDTH * (0.7 + Math.sin(seconds * 0.18) * 0.04);
  const sunY = HEIGHT * 0.3;
  const sun = context.createRadialGradient(sunX, sunY, 2, sunX, sunY, 52);
  sun.addColorStop(0, `rgba(255, 247, 218, ${String(0.92 - colourMix * 0.08)})`);
  sun.addColorStop(1, 'rgba(255, 240, 190, 0)');
  context.fillStyle = sun;
  context.fillRect(sunX - 60, sunY - 60, 120, 120);

  context.fillStyle = rgb(ground);
  context.beginPath();
  context.moveTo(-8, HEIGHT * 0.68);
  context.bezierCurveTo(WIDTH * 0.2, HEIGHT * 0.58, WIDTH * 0.44, HEIGHT * 0.75, WIDTH * 0.63, HEIGHT * 0.63);
  context.bezierCurveTo(WIDTH * 0.78, HEIGHT * 0.55, WIDTH * 0.92, HEIGHT * 0.69, WIDTH + 8, HEIGHT * 0.59);
  context.lineTo(WIDTH + 8, HEIGHT + 8);
  context.lineTo(-8, HEIGHT + 8);
  context.closePath();
  context.fill();

  drawPassingArchitecture(context, seconds, silhouette);
  drawFigures(context, seconds, silhouette);
  context.restore();

  context.save();
  context.globalAlpha = 0.16;
  context.fillStyle = 'rgba(255, 255, 255, 0.85)';
  for (let index = 0; index < 3; index += 1) {
    const scratchSeed = pseudoRandom(frame * 17 + index * 101);
    if (scratchSeed < 0.42) continue;
    const x = scratchSeed * WIDTH;
    context.fillRect(x, 0, 0.7 + pseudoRandom(frame + index) * 1.2, HEIGHT);
  }
  context.restore();

  const grainPixels = grain.data;
  for (let index = 0; index < grainPixels.length; index += 4) {
    const value = Math.floor(pseudoRandom(frame * 9_973 + index) * 255);
    grainPixels[index] = value;
    grainPixels[index + 1] = value;
    grainPixels[index + 2] = value;
    grainPixels[index + 3] = 50;
  }
  grainContext.putImageData(grain, 0, 0);
  context.save();
  context.globalAlpha = 0.22 - colourMix * 0.09;
  context.globalCompositeOperation = 'soft-light';
  context.imageSmoothingEnabled = false;
  context.drawImage(grainContext.canvas, 0, 0, WIDTH, HEIGHT);
  context.restore();

  const vignette = context.createRadialGradient(WIDTH / 2, HEIGHT / 2, HEIGHT * 0.22, WIDTH / 2, HEIGHT / 2, WIDTH * 0.58);
  vignette.addColorStop(0, 'rgba(0, 0, 0, 0)');
  vignette.addColorStop(1, `rgba(0, 0, 0, ${String(0.66 - colourMix * 0.22)})`);
  context.fillStyle = vignette;
  context.fillRect(0, 0, WIDTH, HEIGHT);

  const flicker = 0.94 + pseudoRandom(frame * 37) * 0.08;
  context.fillStyle = `rgba(255, 255, 255, ${String(Math.max(0, flicker - 0.98))})`;
  context.fillRect(0, 0, WIDTH, HEIGHT);
}

function drawPassingArchitecture(context: CanvasRenderingContext2D, seconds: number, colour: Rgb) {
  context.fillStyle = rgb(colour);
  for (let index = 0; index < 5; index += 1) {
    const cycle = ((index * 0.23 + seconds * 0.055) % 1 + 1) % 1;
    const x = WIDTH * (1.15 - cycle * 1.35);
    const width = 20 + index * 8;
    const height = 48 + index * 15;
    context.globalAlpha = 0.16 + index * 0.05;
    context.fillRect(x, HEIGHT * 0.68 - height, width, height);
  }
  context.globalAlpha = 1;
}

function drawFigures(context: CanvasRenderingContext2D, seconds: number, colour: Rgb) {
  const walk = Math.sin(seconds * 2.2) * 4;
  const baseX = WIDTH * 0.39 + Math.sin(seconds * 0.32) * 20;
  const baseY = HEIGHT * 0.77;
  context.fillStyle = rgb(colour);
  drawFigure(context, baseX, baseY, 1, walk);
  context.globalAlpha = 0.78;
  drawFigure(context, baseX + 54, baseY + 4, 0.88, -walk);
  context.globalAlpha = 1;
}

function drawFigure(context: CanvasRenderingContext2D, x: number, y: number, scale: number, walk: number) {
  context.beginPath();
  context.arc(x, y - 70 * scale, 12 * scale, 0, Math.PI * 2);
  context.fill();
  context.beginPath();
  context.moveTo(x - 18 * scale, y - 54 * scale);
  context.quadraticCurveTo(x, y - 68 * scale, x + 18 * scale, y - 54 * scale);
  context.lineTo(x + 12 * scale, y - 12 * scale);
  context.lineTo(x - 12 * scale, y - 12 * scale);
  context.closePath();
  context.fill();
  context.lineWidth = 8 * scale;
  context.lineCap = 'round';
  context.beginPath();
  context.moveTo(x - 5 * scale, y - 15 * scale);
  context.lineTo(x - 8 * scale + walk, y + 15 * scale);
  context.moveTo(x + 5 * scale, y - 15 * scale);
  context.lineTo(x + 10 * scale - walk, y + 15 * scale);
  context.strokeStyle = context.fillStyle;
  context.stroke();
}

type Rgb = [number, number, number];

function mixRgb(from: Rgb, to: Rgb, amount: number): Rgb {
  return [
    Math.round(from[0] + (to[0] - from[0]) * amount),
    Math.round(from[1] + (to[1] - from[1]) * amount),
    Math.round(from[2] + (to[2] - from[2]) * amount),
  ];
}

function rgb(colour: Rgb): string {
  return `rgb(${String(colour[0])} ${String(colour[1])} ${String(colour[2])})`;
}

function pseudoRandom(seed: number): number {
  const value = Math.sin(seed * 12.9898) * 43_758.5453;
  return value - Math.floor(value);
}

function clamp01(value: number): number {
  return Math.min(Math.max(value, 0), 1);
}
