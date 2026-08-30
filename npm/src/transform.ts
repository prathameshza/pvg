import type { Vec2 } from "./types.js";

/**
 * 2D Affine Transformation Matrix.
 */
export class Transform2D {
  constructor(
    public a = 1,
    public b = 0,
    public c = 0,
    public d = 1,
    public tx = 0,
    public ty = 0
  ) {}

  static identity(): Transform2D {
    return new Transform2D(1, 0, 0, 1, 0, 0);
  }

  mul(o: Transform2D): Transform2D {
    return new Transform2D(
      this.a * o.a + this.c * o.b,
      this.b * o.a + this.d * o.b,
      this.a * o.c + this.c * o.d,
      this.b * o.c + this.d * o.d,
      this.a * o.tx + this.c * o.ty + this.tx,
      this.b * o.tx + this.d * o.ty + this.ty
    );
  }

  transformPoint(p: Vec2): Vec2 {
    return [
      this.a * p[0] + this.c * p[1] + this.tx,
      this.b * p[0] + this.d * p[1] + this.ty,
    ];
  }
}