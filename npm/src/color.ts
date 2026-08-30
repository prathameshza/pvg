/**
 * PVG 32-bit RGBA and transparent color primitive.
 */
export class PvgColor {
  public r: number;
  public g: number;
  public b: number;
  public a: number;
  public isNone: boolean;

  constructor(r = 0, g = 0, b = 0, a = 255, isNone = false) {
    this.r = Math.max(0, Math.min(255, Math.round(r)));
    this.g = Math.max(0, Math.min(255, Math.round(g)));
    this.b = Math.max(0, Math.min(255, Math.round(b)));
    this.a = Math.max(0, Math.min(255, Math.round(a)));
    this.isNone = isNone;
  }

  static None(): PvgColor {
    return new PvgColor(0, 0, 0, 0, true);
  }

  static Black(): PvgColor { return new PvgColor(0, 0, 0, 255); }
  static White(): PvgColor { return new PvgColor(255, 255, 255, 255); }
  static Red(): PvgColor { return new PvgColor(255, 0, 0, 255); }
  static Green(): PvgColor { return new PvgColor(0, 255, 0, 255); }
  static Blue(): PvgColor { return new PvgColor(0, 0, 255, 255); }
  static Yellow(): PvgColor { return new PvgColor(255, 255, 0, 255); }
  static Cyan(): PvgColor { return new PvgColor(0, 255, 255, 255); }
  static Magenta(): PvgColor { return new PvgColor(255, 0, 255, 255); }
  static Transparent(): PvgColor { return new PvgColor(0, 0, 0, 0); }

  static fromHex(hex: string): PvgColor | null {
    let s = hex.startsWith("#") ? hex.slice(1) : hex;
    if (s.length === 3) {
      const r = parseInt(s[0] + s[0], 16);
      const g = parseInt(s[1] + s[1], 16);
      const b = parseInt(s[2] + s[2], 16);
      if (isNaN(r) || isNaN(g) || isNaN(b)) return null;
      return new PvgColor(r, g, b, 255);
    }
    if (s.length === 6) {
      const r = parseInt(s.slice(0, 2), 16);
      const g = parseInt(s.slice(2, 4), 16);
      const b = parseInt(s.slice(4, 6), 16);
      if (isNaN(r) || isNaN(g) || isNaN(b)) return null;
      return new PvgColor(r, g, b, 255);
    }
    if (s.length === 8) {
      const r = parseInt(s.slice(0, 2), 16);
      const g = parseInt(s.slice(2, 4), 16);
      const b = parseInt(s.slice(4, 6), 16);
      const a = parseInt(s.slice(6, 8), 16);
      if (isNaN(r) || isNaN(g) || isNaN(b) || isNaN(a)) return null;
      return new PvgColor(r, g, b, a);
    }
    return null;
  }

  toRgbaString(opacityMultiplier = 1.0): string {
    if (this.isNone) return "transparent";
    const effectiveAlpha = Math.max(0, Math.min(1, (this.a / 255.0) * opacityMultiplier));
    return `rgba(${this.r}, ${this.g}, ${this.b}, ${effectiveAlpha})`;
  }

  toSvgString(): string {
    if (this.isNone) return "none";
    if (this.a === 255) {
      const r = this.r.toString(16).padStart(2, "0");
      const g = this.g.toString(16).padStart(2, "0");
      const b = this.b.toString(16).padStart(2, "0");
      return `#${r}${g}${b}`;
    }
    return `rgba(${this.r}, ${this.g}, ${this.b}, ${(this.a / 255.0).toFixed(3)})`;
  }
}