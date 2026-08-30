import { dedentCode, Lexer } from "./lexer.js";
import { Parser } from "./parser.js";
import { Evaluator } from "./evaluator.js";
import {
  detectLoopDuration,
  exportToAnimatedSvgString,
  exportToSvgString,
  renderDrawListToCanvas,
} from "./renderer.js";
import type { DrawList } from "./types.js";

// Global Animation Loop Ticker
class PvgTicker {
  private activeViews = new Set<PvgView>();
  private rafId: number | null = null;
  private lastTimestamp = 0;

  constructor() {
    this.onFrame = this.onFrame.bind(this);
  }

  register(view: PvgView): void {
    this.activeViews.add(view);
    if (!this.rafId && this.activeViews.size > 0 && typeof requestAnimationFrame !== "undefined") {
      this.lastTimestamp = performance.now();
      this.rafId = requestAnimationFrame(this.onFrame);
    }
  }

  unregister(view: PvgView): void {
    this.activeViews.delete(view);
    if (this.activeViews.size === 0 && this.rafId !== null && typeof cancelAnimationFrame !== "undefined") {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
  }

  private onFrame(timestamp: number): void {
    for (const view of this.activeViews) {
      if (view.isConnected && view.isPlaying && view.isVisible) {
        view._handleTick(timestamp);
      }
    }
    if (this.activeViews.size > 0 && typeof requestAnimationFrame !== "undefined") {
      this.rafId = requestAnimationFrame(this.onFrame);
    } else {
      this.rafId = null;
    }
  }
}

const GLOBAL_PVG_TICKER = new PvgTicker();

// SSR-Safe Base Class
const CustomElementBase =
  typeof HTMLElement !== "undefined"
    ? HTMLElement
    : (class {} as unknown as typeof HTMLElement);

/**
 * Standard W3C Custom Element `<pvg-view>` for seamless embedding.
 */
export class PvgView extends CustomElementBase {
  static get observedAttributes(): string[] {
    return [
      "src",
      "code",
      "render",
      "autoplay",
      "loop",
      "fps",
      "time",
      "t",
      "scale",
      "fit",
      "interactive",
      "lazy",
    ];
  }

  private _sourceCode = "";
  private _currentDrawList: DrawList | null = null;
  private _currentTime = 0.0;
  private _startTime = 0.0;
  private _lastFrameTime = 0.0;
  private _isPlaying = false;
  private _isVisible = true;
  private _isAnimatedDoc = false;
  private _manuallySetCode = false;

  private _panX = 0;
  private _panY = 0;
  private _zoom = 1.0;
  private _isDragging = false;
  private _dragStartX = 0;
  private _dragStartY = 0;

  private _viewport!: HTMLElement;
  private _errorOverlay!: HTMLElement;
  private _loadingOverlay!: HTMLElement;
  private _canvas: HTMLCanvasElement | null = null;
  private _ctx: CanvasRenderingContext2D | null = null;

  private _intersectionObserver?: IntersectionObserver;
  private _resizeObserver?: ResizeObserver;
  private _mutationObserver?: MutationObserver;

  constructor() {
    super();
    if (typeof HTMLElement === "undefined" || !this.attachShadow) return;

    this.attachShadow({ mode: "open" });

    this.shadowRoot!.innerHTML = `
      <style>
        :host {
          display: inline-block;
          position: relative;
          width: 100%;
          height: 100%;
          min-width: 60px;
          min-height: 60px;
          overflow: hidden;
          vertical-align: middle;
          contain: layout paint;
          box-sizing: border-box;
        }
        :host([hidden]) {
          display: none !important;
        }
        .pvg-viewport {
          width: 100%;
          height: 100%;
          display: flex;
          align-items: center;
          justify-content: center;
          position: relative;
          overflow: hidden;
        }
        canvas, svg {
          display: block;
          max-width: 100%;
          max-height: 100%;
          touch-action: none;
        }
        svg {
          width: 100%;
          height: 100%;
        }
        .overlay-error {
          position: absolute;
          inset: 0;
          background: rgba(18, 10, 14, 0.92);
          color: #ff4766;
          font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
          font-size: 11px;
          padding: 10px;
          box-sizing: border-box;
          white-space: pre-wrap;
          overflow: auto;
          z-index: 10;
          display: none;
        }
        .overlay-loading {
          position: absolute;
          inset: 0;
          background: rgba(10, 12, 16, 0.5);
          color: #00d2ff;
          font-family: system-ui, -apple-system, sans-serif;
          font-size: 12px;
          display: none;
          align-items: center;
          justify-content: center;
          z-index: 5;
        }
      </style>
      <div class="pvg-viewport" part="viewport">
        <div class="overlay-loading" part="loading">Loading PVG...</div>
        <div class="overlay-error" part="error"></div>
      </div>
    `;

    this._viewport = this.shadowRoot!.querySelector(".pvg-viewport")!;
    this._errorOverlay = this.shadowRoot!.querySelector(".overlay-error")!;
    this._loadingOverlay = this.shadowRoot!.querySelector(".overlay-loading")!;

    this._onMouseDown = this._onMouseDown.bind(this);
    this._onMouseMove = this._onMouseMove.bind(this);
    this._onMouseUp = this._onMouseUp.bind(this);
    this._onWheel = this._onWheel.bind(this);
    this._onDblClick = this._onDblClick.bind(this);
  }

  get isPlaying(): boolean { return this._isPlaying; }
  get isVisible(): boolean { return this._isVisible; }
  get isAnimated(): boolean { return this._isAnimatedDoc; }
  get time(): number { return this._currentTime; }
  set time(val: number) {
    this._currentTime = Number(val) || 0.0;
    this.renderAt(this._currentTime);
  }

  get code(): string { return this._sourceCode; }
  set code(val: string) {
    this._sourceCode = dedentCode(String(val || ""));
    this._manuallySetCode = true;
    this._isAnimatedDoc =
      this._sourceCode.includes("time") ||
      this._sourceCode.includes(" t ") ||
      this._sourceCode.includes("(t)") ||
      this._sourceCode.includes("* t");
    this._setupRenderSurface();
    this.renderAt(this._currentTime);
  }

  get src(): string | null { return this.getAttribute("src"); }
  set src(val: string | null) {
    if (val) this.setAttribute("src", val);
    else this.removeAttribute("src");
  }

  get renderMode(): "canvas" | "svg" {
    return (this.getAttribute("render") || "canvas").toLowerCase() === "svg" ? "svg" : "canvas";
  }
  set renderMode(val: "canvas" | "svg") {
    this.setAttribute("render", val);
  }

  get fit(): "contain" | "cover" {
    return (this.getAttribute("fit") || "contain") as "contain" | "cover";
  }

  get fpsCap(): number {
    const attr = this.getAttribute("fps");
    return attr ? parseInt(attr, 10) : 0;
  }

  connectedCallback(): void {
    if (typeof window === "undefined") return;

    if (window.IntersectionObserver && this.getAttribute("lazy") !== "false") {
      this._intersectionObserver = new IntersectionObserver((entries) => {
        for (const entry of entries) {
          this._isVisible = entry.isIntersecting;
          if (this._isVisible && this._isPlaying) {
            this.renderAt(this._currentTime);
          }
        }
      });
      this._intersectionObserver.observe(this);
    }

    if (window.ResizeObserver) {
      this._resizeObserver = new ResizeObserver(() => {
        if (this.renderMode === "canvas") {
          this._syncCanvasSize();
          this.renderAt(this._currentTime);
        }
      });
      this._resizeObserver.observe(this);
    }

    this._mutationObserver = new MutationObserver(() => {
      if (!this.hasAttribute("src") && !this.hasAttribute("code") && !this._manuallySetCode) {
        this.extractAndCompile();
      }
    });
    this._mutationObserver.observe(this, { childList: true, characterData: true, subtree: true });

    this._viewport.addEventListener("mousedown", this._onMouseDown);
    window.addEventListener("mousemove", this._onMouseMove);
    window.addEventListener("mouseup", this._onMouseUp);
    this._viewport.addEventListener("wheel", this._onWheel as EventListener, { passive: false });
    this._viewport.addEventListener("dblclick", this._onDblClick);

    this.extractAndCompile();

    if (this.hasAttribute("autoplay") || this.hasAttribute("play")) {
      this.play();
    }
  }

  disconnectedCallback(): void {
    GLOBAL_PVG_TICKER.unregister(this);

    this._intersectionObserver?.disconnect();
    this._resizeObserver?.disconnect();
    this._mutationObserver?.disconnect();

    this._viewport?.removeEventListener("mousedown", this._onMouseDown);
    window.removeEventListener("mousemove", this._onMouseMove);
    window.removeEventListener("mouseup", this._onMouseUp);
    this._viewport?.removeEventListener("wheel", this._onWheel as EventListener);
    this._viewport?.removeEventListener("dblclick", this._onDblClick);
  }

  attributeChangedCallback(name: string, oldValue: string | null, newValue: string | null): void {
    if (oldValue === newValue) return;

    if (name === "src") {
      this._fetchSrc(newValue);
    } else if (name === "code") {
      this._sourceCode = dedentCode(newValue || "");
      this._manuallySetCode = true;
      this.extractAndCompile();
    } else if (name === "render") {
      this._setupRenderSurface();
      this.renderAt(this._currentTime);
    } else if (name === "time" || name === "t") {
      this._currentTime = parseFloat(newValue || "0") || 0.0;
      this.renderAt(this._currentTime);
    } else if (name === "autoplay") {
      if (this.hasAttribute("autoplay")) this.play();
      else this.pause();
    } else {
      this.renderAt(this._currentTime);
    }
  }

  play(): void {
    this._isPlaying = true;
    this._startTime = performance.now() - this._currentTime * 1000.0;
    GLOBAL_PVG_TICKER.register(this);
    this.dispatchEvent(new CustomEvent("play", { detail: { time: this._currentTime } }));
  }

  pause(): void {
    this._isPlaying = false;
    GLOBAL_PVG_TICKER.unregister(this);
    this.dispatchEvent(new CustomEvent("pause", { detail: { time: this._currentTime } }));
  }

  togglePlay(): void {
    if (this._isPlaying) this.pause();
    else this.play();
  }

  reset(): void {
    this._startTime = performance.now();
    this._currentTime = 0.0;
    this._panX = 0;
    this._panY = 0;
    this._zoom = 1.0;
    this.renderAt(0.0);
    this.dispatchEvent(new CustomEvent("reset"));
  }

  seek(seconds: number): void {
    this._currentTime = Math.max(0, seconds);
    this._startTime = performance.now() - this._currentTime * 1000.0;
    this.renderAt(this._currentTime);
    this.dispatchEvent(new CustomEvent("seek", { detail: { time: this._currentTime } }));
  }

  exportSvg(options: { animated?: boolean; duration?: number; fps?: number } = {}): string {
    const isAnimated = options.animated !== undefined ? options.animated : this._isAnimatedDoc;
    if (isAnimated && this._sourceCode) {
      const duration = options.duration || detectLoopDuration(this._sourceCode);
      const fps = options.fps || 30;
      return exportToAnimatedSvgString(this._sourceCode, { duration, fps });
    }
    if (!this._currentDrawList) return "";
    return exportToSvgString(this._currentDrawList);
  }

  async toPngBlob(scale = 2): Promise<Blob | null> {
    if (!this._currentDrawList) return null;
    const offscreen = document.createElement("canvas");
    offscreen.width = this._currentDrawList.canvasWidth * scale;
    offscreen.height = this._currentDrawList.canvasHeight * scale;
    const offCtx = offscreen.getContext("2d");
    if (!offCtx) return null;
    renderDrawListToCanvas(offCtx, this._currentDrawList, { zoom: scale });
    return new Promise((resolve) => offscreen.toBlob(resolve, "image/png"));
  }

  getDrawList(): DrawList | null {
    return this._currentDrawList;
  }

  private async _fetchSrc(url: string | null): Promise<void> {
    if (!url) return;
    this._loadingOverlay.style.display = "flex";
    try {
      const resp = await fetch(url);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}: Failed to fetch '${url}'`);
      const text = await resp.text();
      this._sourceCode = dedentCode(text);
      this.extractAndCompile();
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      this._showError(message);
    } finally {
      this._loadingOverlay.style.display = "none";
    }
  }

  extractAndCompile(): void {
    if (this.hasAttribute("src")) return;

    if (!this.hasAttribute("code") && !this._manuallySetCode) {
      const scriptTag = this.querySelector('script[type="text/pvg"], script[type="text/plain"]');
      if (scriptTag) {
        this._sourceCode = dedentCode(scriptTag.textContent || "");
      } else {
        const rawText = this.textContent;
        if (rawText && rawText.trim().length > 0) {
          this._sourceCode = dedentCode(rawText);
        }
      }
    }

    if (!this._sourceCode) return;

    this._isAnimatedDoc =
      this._sourceCode.includes("time") ||
      this._sourceCode.includes(" t ") ||
      this._sourceCode.includes("(t)") ||
      this._sourceCode.includes("* t");

    this._setupRenderSurface();
    this.renderAt(this._currentTime);
  }

  private _setupRenderSurface(): void {
    const mode = this.renderMode;
    this._viewport.innerHTML = "";
    this._viewport.appendChild(this._loadingOverlay);
    this._viewport.appendChild(this._errorOverlay);

    if (mode === "canvas") {
      this._canvas = document.createElement("canvas");
      this._ctx = this._canvas.getContext("2d");
      this._viewport.appendChild(this._canvas);
      this._syncCanvasSize();
    }
  }

  private _syncCanvasSize(): void {
    if (!this._canvas || !this._ctx) return;
    const dpr = parseFloat(this.getAttribute("scale") || "") || window.devicePixelRatio || 1;
    const w = this._viewport.clientWidth || 300;
    const h = this._viewport.clientHeight || 300;
    this._canvas.width = Math.round(w * dpr);
    this._canvas.height = Math.round(h * dpr);
    this._canvas.style.width = `${w}px`;
    this._canvas.style.height = `${h}px`;
    this._ctx.setTransform(1, 0, 0, 1, 0, 0);
    this._ctx.scale(dpr, dpr);
  }

  renderAt(time: number): void {
    if (!this._sourceCode) return;

    const t0 = performance.now();
    try {
      const lexer = new Lexer(this._sourceCode);
      const tokens = lexer.tokenizeAll();
      const parser = new Parser(tokens);
      const ast = parser.parseDocument();
      const evaluator = new Evaluator(time);
      this._currentDrawList = evaluator.evaluateDocument(ast);

      this._hideError();

      if (this.renderMode === "svg") {
        this._renderSvg(this._currentDrawList);
      } else {
        this._renderCanvas(this._currentDrawList);
      }

      const elapsed = performance.now() - t0;
      this.dispatchEvent(
        new CustomEvent("render", {
          detail: {
            drawList: this._currentDrawList,
            time,
            renderTimeMs: elapsed,
          },
        })
      );
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      this._showError(message);
      this.dispatchEvent(new CustomEvent("error", { detail: { error: message } }));
    }
  }

  private _renderCanvas(drawList: DrawList): void {
    if (!this._ctx || !this._canvas) return;

    const w = this._viewport.clientWidth;
    const h = this._viewport.clientHeight;
    this._ctx.clearRect(0, 0, w, h);

    const fit = this.fit;
    let baseZoom = 1.0;
    if (fit === "contain") {
      baseZoom = Math.min(w / drawList.canvasWidth, h / drawList.canvasHeight);
    } else if (fit === "cover") {
      baseZoom = Math.max(w / drawList.canvasWidth, h / drawList.canvasHeight);
    }

    const effectiveZoom = baseZoom * this._zoom;
    const originX = (w - drawList.canvasWidth * effectiveZoom) / 2 + this._panX;
    const originY = (h - drawList.canvasHeight * effectiveZoom) / 2 + this._panY;

    renderDrawListToCanvas(this._ctx, drawList, {
      originX,
      originY,
      zoom: effectiveZoom,
    });
  }

  private _renderSvg(drawList: DrawList): void {
    const existingSvg = this._viewport.querySelector("svg");
    const svgStr = exportToSvgString(drawList);
    if (existingSvg) {
      const parser = new DOMParser();
      const doc = parser.parseFromString(svgStr, "image/svg+xml");
      const newSvg = doc.querySelector("svg");
      if (newSvg) {
        this._viewport.replaceChild(newSvg, existingSvg);
      }
    } else {
      const container = document.createElement("div");
      container.innerHTML = svgStr;
      const svgEl = container.firstElementChild;
      if (svgEl) {
        this._viewport.appendChild(svgEl);
      }
    }
  }

  _handleTick(timestamp: number): void {
    const fps = this.fpsCap;
    if (fps > 0) {
      const frameDuration = 1000.0 / fps;
      if (timestamp - this._lastFrameTime < frameDuration) {
        return;
      }
    }
    this._lastFrameTime = timestamp;

    if (this._isAnimatedDoc) {
      this._currentTime = (timestamp - this._startTime) / 1000.0;
      this.renderAt(this._currentTime);
      this.dispatchEvent(new CustomEvent("timeupdate", { detail: { time: this._currentTime } }));
    }
  }

  private _showError(msg: string): void {
    this._errorOverlay.textContent = `⚡ PVG Execution Error:\n${msg}`;
    this._errorOverlay.style.display = "block";
  }

  private _hideError(): void {
    this._errorOverlay.style.display = "none";
  }

  private _onMouseDown(e: MouseEvent): void {
    if (!this.hasAttribute("interactive")) return;
    this._isDragging = true;
    this._dragStartX = e.clientX - this._panX;
    this._dragStartY = e.clientY - this._panY;
    this._viewport.style.cursor = "grabbing";
  }

  private _onMouseMove(e: MouseEvent): void {
    if (!this._isDragging) return;
    this._panX = e.clientX - this._dragStartX;
    this._panY = e.clientY - this._dragStartY;
    this.renderAt(this._currentTime);
  }

  private _onMouseUp(): void {
    if (this._isDragging) {
      this._isDragging = false;
      this._viewport.style.cursor = this.hasAttribute("interactive") ? "grab" : "default";
    }
  }

  private _onWheel(e: WheelEvent): void {
    if (!this.hasAttribute("interactive")) return;
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.15 : 0.85;
    this._zoom = Math.max(0.05, Math.min(20.0, this._zoom * factor));
    this.renderAt(this._currentTime);
  }

  private _onDblClick(): void {
    if (!this.hasAttribute("interactive")) return;
    this._panX = 0;
    this._panY = 0;
    this._zoom = 1.0;
    this.renderAt(this._currentTime);
  }
}

/**
 * Explicitly registers the `<pvg-view>` custom element in the DOM.
 */
export function registerPvgView(tagName = "pvg-view"): void {
  if (typeof customElements !== "undefined" && !customElements.get(tagName)) {
    customElements.define(tagName, PvgView);
  }
}

// Auto-register in browser environments
if (typeof customElements !== "undefined" && !customElements.get("pvg-view")) {
  customElements.define("pvg-view", PvgView);
}