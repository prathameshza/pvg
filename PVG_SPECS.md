# PVG 0.2 Language & Architecture Specification

**PVG (Procedural Vector Graphics)** is a deterministic, human-readable 2D vector graphics and procedural scene description language. It is designed to combine the declarative clarity of vector graphics with the programmatic power of a sandboxed procedural language while maintaining **microsecond CPU evaluation, zero GPU dependency, and a sub-50KB memory footprint**.

## What's New in 0.2

PVG 0.1 covered flat-color vector geometry. PVG 0.2 adds the five rendering essentials for game-ready assets with depth, weight, and visual punch:

1. **Stroke topology** — `cap` (`butt`/`round`/`square`), `join` (`miter`/`round`/`bevel`), `miter` limit, `dash` patterns, and stroke `align` (`center`/`inside`/`outside`) (Section 8).
2. **Gradients** — `linear`, `radial` (with focal point), and `angular`/`conic` paints with `stop` color lists (Section 9).
3. **Lighting & depth FX** — O(N) CPU `blur`, offset `shadow`, and additive `glow` (Section 10).
4. **Clipping & masks** — declarative `clip` blocks whose first shape bounds all content (Section 11).
5. **Blend modes** — `blend "normal" | "add" | "multiply" | "screen" | "overlay"` (Section 12).

**Versioning policy:** a document declares `PVG 0.1` or `PVG 0.2` in its header (Section 2.1). Readers implementing 0.2 **must** accept both headers; every valid 0.1 document evaluates identically under 0.2 (new properties default to 0.1 behavior: `butt` caps, `miter` joins, solid paints, `normal` blending, no filters, no clipping). Authors should declare `PVG 0.2` when using 0.2 features. The words `clip`, `cap`, `join`, `miter`, `dash`, `blend`, `blur`, `shadow`, `glow`, `linear`, `radial`, `angular`, `conic`, and `stop` are reserved keywords in 0.2 (Section 2.7) and can no longer be used as variable or function names.

---

## 1. Design Principles & Goals

```
[ PVG 0.1 / 0.2 Source ]
        │
        ▼ (Single-Pass Tokenizer & Recursive Descent Parser)
[ Abstract Syntax Tree (AST) ] (~10–25 KB)
        │
        ▼ (Procedural Evaluator: Loops, Math, Typography, Scope Resolution)
[ Flat 2D Draw List (Contiguous Structs) ] (~15–35 KB)
[ Paints (solid / linear / radial / angular), Stroke Topology, FX, Clip Masks ]
        │
   ┌────┴───────────────────────────┬────────────────────────────┐
   ▼                                ▼                            ▼
[ Native GUI Painter ]     [ Standalone SVG ]          [ CPU Software Rasterizer ]
(egui / Skia / Window)     (W3C SMIL Animation)        (1-Line Scanline Buffer)
```

1. **Procedural Native:** Native support for variables, dynamic loops, user-defined functions, typography, trigonometry, and mathematical expressions directly in the graphics definition.
2. **Deterministic & Pure:** Identical source text evaluates to the identical scene graph across all operating systems, CPU architectures, and runtime backends.
3. **Ultra-Low Resource Footprint:** Compiles and evaluates inside `< 50 KB` of heap memory with execution times under $0.2\text{ ms}$ per frame on a single CPU thread.
4. **No XML/DOM Overhead:** Eliminates deeply nested closing tags, XML namespaces, and heavy browser DOM hierarchies.
5. **Algebraic Curve & Text Simplicity:** Replaces SVG's heavy endpoint-to-center elliptical arc matrix inversions with direct forward trigonometry and provides a zero-overhead text and font layout primitive.

---

## 2. Lexical Grammar

### 2.1 File Encoding & Structure
* **Encoding:** UTF-8. Multi-byte Unicode characters in string literals (e.g. `°`, `©`, `™`, emojis) are natively decoded without byte fragmentation.
* **Header:** A PVG document **must** begin with the header `PVG <major>.<minor>` on the first non-empty line (`PVG 0.1` or `PVG 0.2`). The version is informational: 0.2 readers parse both and evaluate 0.1 documents identically to 0.1 readers. New 0.2 properties are accepted regardless of the declared version, but authors should declare `PVG 0.2` when using them so older tooling can report a clean version diagnostic instead of a syntax error.
* **Line Termination:** Newlines (`\n` or `\r\n`) terminate statements.
* **Indentation:** Exactly **2 spaces** per indentation level. Tabs (`\t`) are forbidden to eliminate cross-platform layout ambiguity.
* **Comments:** Single-line comments begin with `#` and continue to the end of the line. Full-line comments and trailing comments are supported.

### 2.2 Identifiers
Identifiers represent variable names, function names, and keywords:
```ebnf
identifier = ( letter | "_" ) , { letter | digit | "_" | "-" } ;
```
* **Valid:** `cx`, `outer_r`, `step_len`, `player_1`, `main-track`
* **Invalid:** `123start`, `center+offset`

### 2.3 Numbers & Unit Suffixes
PVG numbers are 64-bit IEEE 754 floating-point values:
```ebnf
number = [ "+" | "-" ] , digit , { digit } , [ "." , { digit } ] , [ unit ] ;
unit   = "deg" | "rad" ;
```
* **Degrees Suffix (`deg`):** Automatically converts degrees to radians at parse time:
  $$\theta_{\text{rad}} = \theta_{\text{deg}} \times \frac{\pi}{180}$$
  *Example:* `180deg` evaluates directly to `3.141592653589793`.
* **Radians Suffix (`rad`):** Expresses raw radians (e.g., `1.5rad`).

### 2.4 String Literals
Strings are enclosed in double quotes (`"..."`) and support standard escape sequences (`\n`, `\t`, `\r`, `\"`, `\\`):
```ebnf
string = '"' , { character | escape_sequence } , '"' ;
```
* *Examples:* `"ENGINE SPEED"`, `"Telemetry Active\nStatus: OK"`, `"60 °C"`
* **Triple-quoted blocks** (`"""..."""`) are raw multi-line literals (no escapes): the opener must end its line, the closer must be alone on its line (a trailing `# comment` is allowed on both), and continuation lines are dedented by the opener's indent. Ideal for sprite art and text blocks (Section 18.3).

### 2.5 2D Coordinate & Vector Literals
To eliminate operator ambiguity with arithmetic expressions, all 2D vector coordinates **must** use bracket delimiters:
```ebnf
vector2 = "[" , expression , "," , expression , "]" ;
```
* *Examples:* `[100, 200]`, `[cx + r * cos(a), cy + r * sin(a)]`, `[x - 10, y + 5]`

### 2.6 Color Literals
* **Hex Color:** `#RGB`, `#RRGGBB`, `#RRGGBBAA` (e.g., `#fff`, `#00ffcc`, `#ff005580`). The last two hex digits of the 8-digit form are the alpha channel (`00` = fully transparent, `ff` = opaque).
* **Keywords:** `black`, `white`, `red`, `green`, `blue`, `yellow`, `cyan`, `magenta`, `none`, `transparent`.
* **Functional Form:** `rgb(r, g, b)` and `rgba(r, g, b, a)` where $r, g, b \in [0, 255]$ and $a \in [0.0, 1.0]$. Components are rounded to the nearest integer and clamped into range ($r, g, b$ into $[0, 255]$, $a$ into $[0.0, 1.0]$ before mapping to a byte); wrong arities and non-numeric components are runtime errors.
* **Transparent stops are meaningful:** a gradient stop such as `stop 1.0 #0066ff00` (alpha `00`) is a genuine fade-out endpoint, not an empty value. Renderers must interpolate the alpha channel across stops (Section 9.4).

### 2.7 Reserved Keywords (0.2)
The following words are keywords in 0.2 and cannot be used as variable, function, or loop-variable names: `clip`, `cap`, `join`, `miter`, `dash`, `blend`, `blur`, `shadow`, `glow`, `linear`, `radial`, `angular`, `conic`, `stop`. (The word `conic` is accepted as an alias of `angular`.) A 0.1 document that uses any of these as an identifier must rename it before declaring `PVG 0.2`.

---

## 3. Formal EBNF Grammar

```ebnf
Document        ::= Header CanvasDecl ParamDecl* PatternDef* Statement* EOF ;
Header          ::= "PVG" Version NEWLINE ;
Version         ::= NUMBER "." NUMBER ;

CanvasDecl      ::= "canvas" NUMBER NUMBER NEWLINE
                    ( INDENT CanvasProp+ DEDENT )? ;
CanvasProp      ::= ( "background" Color
                    | "snap" NUMBER
                    | "filter" ( STRING | IDENTIFIER ) ) NEWLINE ;
(* Section 18.3: `snap 1.0` rounds coords to the pixel grid (0 = off);
   `filter "nearest"` = crisp pixels, `"linear"` (default) = smooth. *)

(* Section 18.1: host-overridable uniforms (top level, or `set`-like in blocks). *)
ParamDecl       ::= "param" IDENTIFIER ( ":" | "=" ) Expression NEWLINE ;

(* Section 18.4: repeatable tile (top level only). *)
PatternDef      ::= "pattern" IDENTIFIER NUMBER NUMBER NEWLINE Block ;

Statement       ::= SetStmt
                  | ForStmt
                  | WhileStmt
                  | IfStmt
                  | DefStmt
                  | CallStmt
                  | ReturnStmt
                  | SeedStmt
                  | CircleStmt
                  | EllipseStmt
                  | RectStmt
                  | LineStmt
                  | PolygonStmt
                  | TextStmt
                  | PathStmt
                  | GroupStmt
                  | SpriteStmt     (* Section 18.3: pixel-art sprite *)
                  | SplineStmt     (* Section 18.5: Catmull-Rom data spline *)
                  | ClipStmt ;   (* 0.2: clipping / masking block *)

SetStmt         ::= "set" IDENTIFIER "=" Expression NEWLINE ;
SeedStmt        ::= "seed" NUMBER NEWLINE ;
ReturnStmt      ::= "return" Expression NEWLINE ;
CallStmt        ::= IDENTIFIER "(" ( Expression ( "," Expression )* )? ")" NEWLINE ;

ForStmt         ::= "for" IDENTIFIER "from" Expression "to" Expression ( "step" Expression )? NEWLINE Block ;
WhileStmt       ::= "while" Expression NEWLINE Block ;
IfStmt          ::= "if" Expression NEWLINE Block ( "else" ( IfStmt | NEWLINE Block ) )? ;
DefStmt         ::= "def" IDENTIFIER "(" ( IDENTIFIER ( "," IDENTIFIER )* )? ")" NEWLINE Block ;

Block           ::= INDENT Statement+ DEDENT ;

CircleStmt      ::= "circle" NEWLINE INDENT CircleProp+ DEDENT ;
CircleProp      ::= ( "center" Vector2 | "radius" Expression | StyleProp ) NEWLINE ;

EllipseStmt     ::= "ellipse" NEWLINE INDENT EllipseProp+ DEDENT ;
EllipseProp     ::= ( "center" Vector2 | "radius" Vector2 | StyleProp ) NEWLINE ;

RectStmt        ::= ( "rectangle" | "rect" ) NEWLINE INDENT RectProp+ DEDENT ;
RectProp        ::= ( "pos" Vector2 | "size" Vector2 | "radius" Expression | StyleProp ) NEWLINE ;

LineStmt        ::= "line" NEWLINE INDENT LineProp+ DEDENT ;
LineProp        ::= ( "from" Vector2 | "to" Vector2 | StyleProp ) NEWLINE ;

PolygonStmt     ::= "polygon" NEWLINE INDENT PolygonProp+ DEDENT ;
PolygonProp     ::= ( "points" Vector2+ | StyleProp ) NEWLINE ;

TextStmt        ::= "text" NEWLINE INDENT TextProp+ DEDENT ;
TextProp        ::= ( "pos" Vector2 | "content" Expression | "size" Expression | "font" Expression | "align" Expression | TextStyleProp ) NEWLINE ;
(* NOTE: `align` on `text` is the horizontal text anchor ("left" | "center" |
   "right"), NOT stroke alignment. `text` does not accept `cap`, `join`,
   `miter`, or `dash`. *)

PathStmt        ::= "path" NEWLINE INDENT ( PathCommand | StyleProp | SetStmt )+ DEDENT ;
PathCommand     ::= ( "start" Vector2
                    | "line"  Vector2
                    | "quad"  Vector2 Vector2
                    | "curve" Vector2 Vector2 Vector2
                    | "arc"   Vector2 Expression Expression Expression
                    | "close"
                    | PathFor | PathWhile | PathIf ) NEWLINE ;
PathFor         ::= "for" IDENTIFIER "from" Expression "to" Expression ( "step" Expression )? NEWLINE PathBlock ;
PathWhile       ::= "while" Expression NEWLINE PathBlock ;
PathIf          ::= "if" Expression NEWLINE PathBlock ( "else" ( PathIf | NEWLINE PathBlock ) )? ;
PathBlock       ::= INDENT PathCommand+ DEDENT ;   (* style props not allowed inside *)

GroupStmt       ::= "group" NEWLINE INDENT ( GroupProp | Statement )+ DEDENT ;
GroupProp       ::= ( "pos" Vector2 | "rot" Expression | "scale" Vector2
                    | "opacity" Expression | "fill" PaintExpr | "stroke" PaintExpr
                    | "blend" Expression | "blur" Expression
                    | "shadow" Vector2 Expression Expression
                    | "glow" Expression Expression ) NEWLINE ;
TextStyleProp   ::= "fill" PaintExpr | "stroke" PaintExpr | "width" Expression
                  | "opacity" Expression | "blur" Expression
                  | "shadow" Vector2 Expression Expression
                  | "glow" Expression Expression | "blend" Expression ;

(* Section 18.3: indexed-color pixel sprite. `palette` is a bracketed color
   list; each `data`/`row` string is one pixel row (`0`-`9`/`a`-`z` index the
   palette, `.`/space = transparent); a `"""` block holds many rows. *)
SpriteStmt      ::= "sprite" NEWLINE INDENT SpriteProp+ DEDENT ;
SpriteProp      ::= ( "pos" Vector2
                    | "palette" "[" Expression ( "," Expression )* "]"
                    | ( "data" | "row" ) STRING
                    | "scale" Expression | "opacity" Expression
                    | "blend" Expression ) NEWLINE ;

(* Section 18.5: smooth spline through an array of `[x, y]` points, or of bare
   numbers auto-laid-out across `pos`/`size`. *)
SplineStmt      ::= "spline" NEWLINE INDENT SplineProp+ DEDENT ;
SplineProp      ::= ( "points" Expression | "pos" Vector2 | "size" Vector2
                    | "stroke" PaintExpr | "width" Expression
                    | "opacity" Expression | "cap" Expression
                    | "join" Expression | "miter" Expression
                    | "dash" DashArray | "blur" Expression
                    | "shadow" Vector2 Expression Expression
                    | "glow" Expression Expression
                    | "blend" Expression ) NEWLINE ;

(* 0.2: a clip block. The FIRST shape is the mask; all following
   statements render confined to it. See Section 11. *)
ClipStmt        ::= "clip" NEWLINE INDENT ShapeStmt ( Statement )* DEDENT ;
ShapeStmt       ::= CircleStmt | EllipseStmt | RectStmt | LineStmt
                  | PolygonStmt | PathStmt | TextStmt ;

(* 0.2: paints — solid colors or gradients with nested stops. See Section 9. *)
StyleProp       ::= "fill" PaintExpr
                  | "stroke" PaintExpr
                  | "width" Expression
                  | "opacity" Expression
                  | "cap" Expression
                  | "join" Expression
                  | "miter" Expression
                  | "dash" DashArray
                  | "align" Expression
                  | "blur" Expression
                  | "shadow" Vector2 Expression Expression
                  | "glow" Expression Expression
                  | "blend" Expression ;

PaintExpr       ::= LinearGrad | RadialGrad | AngularGrad | PatternRef | Expression ;
(* Section 18.4: `fill pattern name` references a top-level tile. A bare variable
   literally named `pattern` still parses as an expression elsewhere. *)
PatternRef      ::= "pattern" IDENTIFIER ;

LinearGrad      ::= "linear" Vector2 Vector2 NEWLINE ( INDENT StopDecl+ DEDENT )? ;
RadialGrad      ::= "radial" Vector2 Expression ( Vector2 )? NEWLINE ( INDENT StopDecl+ DEDENT )? ;
AngularGrad     ::= ( "angular" | "conic" ) Vector2 Expression NEWLINE ( INDENT StopDecl+ DEDENT )? ;
StopDecl        ::= "stop" Expression Expression NEWLINE ;

DashArray       ::= "[" Expression ( "," Expression )* "]" ;

Vector2         ::= "[" Expression "," Expression "]" ;
ArrayLit        ::= "[" [ Expression ( "," Expression )* [ "," ] ] "]" ;
(* Section 18.5: a bracket list with arity ≠ 2 is an Array; exactly 2 *scalar*
   components stay a Vec2 (back-compat for positions); nested compounds
   (`[[10, 10], [90, 90]]`) are Arrays. `array(a, b)` builds an explicit
   2-element Array. *)
```

**Conventions used above:**
* `IDENTIFIER` also matches the post-0.2 *soft* keywords (`param`, `pattern`, `sprite`, `spline`, `palette`, `data`, `row`, `snap`, `filter`) wherever a name is expected — e.g. `for row from 0 to 7` stays valid; their special meaning applies only where their syntax applies (Section 18).
* `PaintExpr` falls back to a plain `Expression` (solid color or computed value) when the next token is not `linear` / `radial` / `angular` / `conic` / `pattern`, so every 0.1 `fill`/`stroke` line is still a valid `PaintExpr`.
* `shadow` takes three operands: offset `Vector2`, blur-radius `Expression`, color `Expression` (e.g., `shadow [4, 6] 8 #00000080`).
* `glow` takes two operands: blur-radius `Expression` and color `Expression` (e.g., `glow 12 #00ffcc`).
* `dash` takes a bracketed list of lengths (e.g., `dash [28, 8, 12, 8]`); an empty list paints solid.

---

## 4. Expression Syntax & Operator Precedence

Expressions are evaluated dynamically from highest to lowest precedence:

| Precedence | Operators | Description | Associativity |
| :--- | :--- | :--- | :--- |
| **1 (Highest)**| `()` | Grouping parentheses | Left |
| **2** | `^` | Exponentiation / Power | Right |
| **3** | `-` (unary), `not` | Negation, Logical NOT | Right |
| **4** | `*`, `/`, `%` | Multiplication, Division, Modulus | Left |
| **5** | `+`, `-` | Addition / String Concatenation, Subtraction | Left |
| **6** | `<`, `<=`, `>`, `>=` | Relational comparisons | Left |
| **7** | `==`, `!=` | Equality / Inequality | Left |
| **8** | `and` | Logical AND | Left |
| **9** | `or` | Logical OR | Left |
| **10 (Lowest)**| `? :` | Ternary conditional | Right |

### 4.1 String Concatenation & Automatic Coercion
The addition operator `+` serves as a high-performance string concatenation operator when either operand is a `String`. Numbers and booleans coerce automatically:
```pvg
set speed = 3200
set label = "Speed: " + speed + " RPM"     # Evaluates to "Speed: 3200 RPM"
set status = "System OK: " + true           # Evaluates to "System OK: true"
```

### 4.2 Built-in Constants & Math Functions
* **Constants:** `PI` ($3.14159265...$), `TAU` ($6.28318530...$), `deg_to_rad` ($\pi/180$), `rad_to_deg` ($180/\pi$).
* **Trigonometry:** `sin(x)`, `cos(x)`, `tan(x)`.
* **Algebraic & Rounding:** `sqrt(x)`, `abs(x)`, `pow(base, exp)`, `floor(x)`, `ceil(x)`, `round(x)`.
* **Clamping & Min/Max:** `min(a, b)`, `max(a, b)`.
* **Angle Conversion:** `radians(deg)`, `degrees(rad)`, `deg_to_rad(deg)`.
* **RNG:** `random(min, max)` returns a deterministic pseudorandom float in $[min, max]$.
* **Deterministic Noise (Section 18.2):** `noise2d(x, y)` / `noise3d(x, y, z)` return value noise in $[-1, 1]$, pure functions of their coordinates (no RNG state).
* **Arrays (Section 18.5):** `array(...)` builds an explicit array, `len(arr)` counts elements (or string characters), `get(arr, i)` indexes with negative-wrap.

---

## 5. Procedural Execution & Scoping Model

### 5.1 Dynamic Typing
The runtime supports 8 primitive value types (0.2 adds `Paint`, post-0.2 adds `Array`):
1. `Number(f64)`
2. `Bool(bool)`
3. `String(String)`
4. `Color(Color)`
5. `Vec2(f64, f64)`
6. `Array(Vec<Value>)` — 1D data list for charts, waves, and spline control values (Section 18.5).
7. `Paint(Paint)` — a solid color, an evaluated `linear` / `radial` / `angular` gradient (see Section 9), or a `pattern` tile reference (see Section 18.4). Anywhere a `Color` is accepted, a `Paint` holding a solid color is accepted too.
8. `None`

### 5.2 Lexical Scoping & State Isolation
* **Global Scope:** Variables declared via `set` at the root document level are available globally.
* **Function Scope:** Function definitions create a distinct local activation frame. Arguments and local variables shadow outer variables.
* **Path & Group Scope:** `path` and `group` blocks inherit current variable scopes and can execute nested `set` statements without mutating parent state unless explicitly shadowed. Post-0.2, `path` bodies may also contain `for` / `while` / `if` control flow sharing the path's locals scope (Section 18.6).

```pvg
set global_radius = 50

def make_ring(cx, cy, r)
  set local_width = r * 0.1
  circle
    center [cx, cy]
    radius r
    fill none
    stroke #ffffff
    width local_width

make_ring(300, 300, global_radius)
```

### 5.3 Deterministic Random Number Generation
PVG specifies a 64-bit Xorshift pseudorandom generator:
```pvg
seed 42891

for i from 0 to 10
  set offset = random(-10, 10)
  circle
    center [100 + i * 20, 200 + offset]
    radius 5
```

---

## 6. 2D Geometry & Typography Primitives

All visual nodes inherit parent `group` styles unless explicitly overridden. In 0.2, `fill` and `stroke` accept any `PaintExpr` (solid color or gradient, Section 9), and every geometric primitive additionally accepts the Section 8 stroke-topology properties (`cap`, `join`, `miter`, `dash`, `align`), the Section 10 filter properties (`blur`, `shadow`, `glow`), and the Section 12 `blend` mode. `text` accepts `blur`, `shadow`, `glow`, and `blend` (its `align` remains the text anchor).

### 6.1 Circle
```pvg
circle
  center [cx, cy]       # Mandatory 2D center coordinate
  radius r              # Mandatory scalar radius
  fill paint            # Optional (default: black; solid color or gradient)
  stroke paint          # Optional (default: none)
  width number          # Optional stroke width in pixels (default: 1.0)
  opacity number        # Optional alpha multiplier in [0.0, 1.0] (default: 1.0)
  cap "butt"            # 0.2: line caps (default: "butt")
  join "miter"          # 0.2: line joins (default: "miter")
  miter number          # 0.2: miter limit (default: 4.0)
  dash [a, b, ...]      # 0.2: dash/gap pattern (default: solid)
  align "center"        # 0.2: stroke positioning (default: "center")
  blur number           # 0.2: object blur radius in px (default: 0)
  shadow [dx, dy] r color  # 0.2: drop shadow (default: none)
  glow r color          # 0.2: outer glow (default: none)
  blend "normal"        # 0.2: blend mode (default: "normal")
```

### 6.2 Ellipse
```pvg
ellipse
  center [cx, cy]       # 2D center coordinate
  radius [rx, ry]       # 2D semi-major and semi-minor radii
```

### 6.3 Rectangle
```pvg
rectangle
  pos [x, y]            # Top-left anchor position
  size [width, height]  # Dimensions
  radius r              # Optional uniform corner radius
```

### 6.4 Line
```pvg
line
  from [x1, y1]         # Start coordinate
  to   [x2, y2]         # End coordinate
  stroke color
  width stroke_width
```

### 6.5 Polygon
```pvg
polygon
  points [x1, y1] [x2, y2] [x3, y3] ... # Array of vertex coordinates
  fill color
  stroke color
```

### 6.6 Text & Typography
```pvg
text
  pos [x, y]            # Mandatory anchor position coordinate
  content string/expr   # Mandatory text content (string literal or evaluated expression)
  size number           # Optional font size in pixels (default: 16.0)
  font string           # Optional font family ("sans", "mono", "serif", or custom; default: "sans-serif")
  align string          # Optional horizontal alignment ("left", "center", "right"; default: "left")
  fill paint            # Optional text fill (default: black; solid or gradient)
  opacity number        # Optional alpha multiplier in [0.0, 1.0] (default: 1.0)
  blur number           # 0.2: text blur (default: 0)
  shadow [dx, dy] r color  # 0.2: text drop shadow (default: none)
  glow r color          # 0.2: text glow (default: none)
  blend "normal"        # 0.2: text blend mode (default: "normal")
```

#### Typography Layout Specifications
* **Vertical Baseline:** Conforms to `dominant-baseline="hanging"` / `top-aligned` relative to `pos [x, y]`.
* **Horizontal Anchor:**
  * `"left"`: Positional `[x, y]` represents the top-left boundary of the text.
  * `"center"`: Positional `[x, y]` represents the top-center anchor.
  * `"right"`: Positional `[x, y]` represents the top-right boundary.
* **Transform Inheritance:** Text anchors rotate, scale, and translate deterministically under parent `group` transforms.

---

## 7. The Lean Path System

SVG's path syntax is bloated and relies on complex elliptical arc conversions. PVG replaces this with **6 explicit, low-CPU sub-commands**.

```pvg
path
  fill color
  stroke color
  width stroke_width
  opacity number

  set local_var = 100               # Local calculations inside paths
  start [x, y]                      # Move to point (subpath start)
  line  [x, y]                      # Straight line segment
  quad  [cx, cy] [x, y]             # Quadratic Bézier (control point, endpoint)
  curve [c1x, c1y] [c2x, c2y] [x, y]# Cubic Bézier (control 1, control 2, endpoint)
  arc   [cx, cy] r start_deg end_deg# Center-radius circular arc
  close                             # Closes subpath back to 'start'

  for deg from 0 to 360 step 6      # Control flow (for/while/if, nestable)
    set pt = [cx + r * cos(rad), cy + r * sin(rad)]
    if deg == 0                     # (style props stay in the outer body)
      start pt
    else
      line pt
```

### 7.1 Mathematical Evaluation of Path Primitives

#### Quadratic Bézier (`quad`)
Parametric formula evaluated from $t = 0 \to 1$:
$$\mathbf{B}(t) = (1-t)^2 \mathbf{P}_0 + 2(1-t)t \mathbf{P}_1 + t^2 \mathbf{P}_2$$

#### Cubic Bézier (`curve`)
Parametric formula evaluated from $t = 0 \to 1$:
$$\mathbf{B}(t) = (1-t)^3 \mathbf{P}_0 + 3(1-t)^2 t \mathbf{P}_1 + 3(1-t) t^2 \mathbf{P}_2 + t^3 \mathbf{P}_3$$

#### Center-Radius Circular Arc (`arc`)
Evaluated directly via forward trigonometry without iterative matrix inversion:
$$x(\theta) = c_x + r \cdot \cos(\theta), \quad y(\theta) = c_y + r \cdot \sin(\theta) \quad (\theta \in [\theta_{\text{start}}, \theta_{\text{end}}])$$

---

## 8. Stroke Topology & Crisp Edges (0.2)

Without explicit stroke geometry, renderers must guess how segments meet — producing mushy corners where a razor-sharp mechanical edge was intended. 0.2 exposes the full stroke model on every geometric primitive (and on `path`):

```pvg
line
  from [20, 100]
  to   [300, 100]
  stroke #00d2ff
  width 4
  cap "butt"            # "butt" (default) | "round" | "square"
  join "miter"          # "miter" (default) | "round" | "bevel"
  miter 4.0             # Miter limit, clamped to >= 1.0 (default: 4.0)
  dash [28, 8, 12, 8]   # Dash/gap lengths in px, cycling (default: solid)
  align "center"        # "center" (default) | "inside" | "outside"
```

### 8.1 Semantics
* **`cap`:** Shape of open-subpath endpoints. `"butt"` ends exactly at the endpoint; `"round"` adds a semicircular cap; `"square"` adds a half-width rectangular cap.
* **`join`:** Shape of segment joints. `"miter"` extends edges to a sharp point (cut off at the `miter` limit, falling back toward a bevel for extreme angles); `"round"` rounds the joint; `"bevel"` cuts it flat.
* **`dash`:** Alternating on/off lengths along the path, starting with "on" at phase 0 and cycling. An empty list (the default) means a solid stroke. Non-positive entries are ignored; a fully non-positive list renders solid.
* **`align`:** Stroke positioning relative to the geometric edge. `"center"` straddles the edge (0.1 behavior); `"inside"` / `"outside"` offset the stroke band inward / outward. (On `text`, the word `align` instead keeps its 0.1 meaning: the horizontal text anchor.)
* Defaults reproduce 0.1 rendering exactly: `cap "butt"`, `join "miter"`, `miter 4.0`, solid dash, `align "center"`.

---

## 9. Gradients (0.2)

`fill` and `stroke` accept gradient paints with nested `stop` declarations. Offsets are expressions in $[0.0, 1.0]$ (clamped); colors are any color expression, **including transparent ones** (`#rrggbbaa` with low alpha is a genuine fade-out endpoint, Section 2.6). Stops are sorted by offset at evaluation; an empty stop list paints transparent; a single stop paints solid; a degenerate linear gradient (`start == end`) paints the last stop's color.

### 9.1 Linear Gradients (directional light & shading)
```pvg
# linear [start] [end], then one stop per line:
rectangle
  pos [106, 106]
  size [300, 300]
  fill linear [106, 106] [406, 406]
    stop 0.0 #2b3040
    stop 0.5 #171922
    stop 1.0 #0e1017
```
Parameter: $$t = \mathrm{clamp}\left(\frac{(\mathbf{p} - \mathbf{s}) \cdot (\mathbf{e} - \mathbf{s})}{\lVert \mathbf{e} - \mathbf{s} \rVert^2}, 0, 1\right)$$

### 9.2 Radial Gradients (spheres, gems, spotlights)
```pvg
# radial [center] radius ([focal])?, then stops:
circle
  center [cx, cy]
  radius 90
  fill radial [cx, cy] 90
    stop 0.0 #00ffff
    stop 0.6 #0033aa
    stop 1.0 #07090e
```
An optional focal point `[fx, fy]` after the radius offsets the highlight (two-point conical, $r_0 = 0$ at the focal point). Without it, $t = \mathrm{clamp}(\lVert \mathbf{p} - \mathbf{c} \rVert / r, 0, 1)$. With focal $\mathbf{f}$ and center $\mathbf{c}$ ($R$ = radius), $t$ solves $\lVert \mathbf{d} - t\mathbf{v} \rVert = tR$ with $\mathbf{d} = \mathbf{p} - \mathbf{f}$, $\mathbf{v} = \mathbf{c} - \mathbf{f}$ (positive root, clamped).

### 9.3 Angular / Conic Gradients (cooldowns, sweeps)
```pvg
# angular [center] start_angle (`conic` is an alias), then stops:
circle
  center [cx, cy]
  radius 60
  fill angular [cx, cy] 0deg
    stop 0.0 #00ffcc
    stop 0.75 #00ffcc
    stop 1.0 transparent
```
Parameter: $$t = \mathrm{frac}\left(\frac{\mathrm{atan2}(p_y - c_y,\ p_x - c_x) - \theta_0}{2\pi}\right)$$

### 9.4 Gradient Evaluation Rules
* Gradient control geometry (endpoints, center, radius, focal) is evaluated in **world space**: it follows parent `group` transforms deterministically, exactly like geometry.
* Stop offsets and colors are full expressions — they may reference `time`, loop variables, and function results.
* Color (including alpha) is interpolated linearly between adjacent stops in straight (non-premultiplied) RGBA.

---

## 10. Blur, Glow & Shadows (0.2)

Naive 2D gaussian convolution is $O(R^2)$ per pixel. PVG constrains filters to operations implementable as $O(N)$ separable passes (reference: 3-pass box blur approximating a gaussian with $\sigma \approx r$), keeping typical game assets ($256$–$512$ px) under a millisecond per filtered shape on a single CPU thread.

```pvg
circle
  center [cx, cy]
  radius 160
  fill #00aaff
  blur 45              # Direct object blur, radius in px (default: 0)
  blend "add"
  opacity 0.25

rectangle
  pos [106, 106]
  size [300, 300]
  fill #171922
  shadow [0, 18] 24 #000000bb   # offset [dx, dy], blur radius, color

circle
  center [cx, cy]
  radius 112
  fill none
  stroke #00d2ff
  glow 8 #00d2ff80              # blur radius, color
```

### 10.1 Semantics & Compositing Order
* **`blur r`:** The shape (fill + stroke) is rendered to a layer, blurred, and composited. The blur *replaces* the sharp shape.
* **`shadow [dx, dy] r color`:** A blurred silhouette in the shadow color is drawn at the offset **underneath** the sharp shape (SourceOver).
* **`glow r color`:** A blurred silhouette in the glow color is composited **additively** underneath the sharp shape — the neon/energy halo; the shape itself stays crisp on top.
* Combinations stack in the order shadow → glow → shape (or blurred shape when `blur` is present).
* Radii are clamped to $\ge 0.0$. Blur layers are padded by $\approx 3r$ on all sides so gaussian tails die smoothly instead of clipping into a square seam.

---

## 11. Clipping & Masks (0.2)

A `clip` block confines drawing to a geometric boundary — segmented health bars, circular radar viewports, liquid fills — without manual polygon math:

```pvg
# Everything after the first shape is clipped to it:
clip
  # 1. Mask shape (any geometric primitive)
  rectangle
    pos [100, 100]
    size [200, 20]
    radius 6

  # 2. Clipped content (loops, groups, even nested clips allowed)
  for s from 0 to 15
    line
      from [80 + s * 20, 90]
      to   [110 + s * 20, 130]
      stroke #00e676
      width 6
```

### 11.1 Semantics
* The **first** statement must be a geometric shape (`circle`, `ellipse`, `rectangle`, `polygon`, `path`, `line`, `text`); anything else is a parse error. It is evaluated but never drawn — it only defines the mask.
* Every subsequent statement is evaluated normally, then composited **through the intersection** with the mask. Content may include loops, function calls, groups, and nested `clip` blocks (nested masks intersect).
* The mask follows the same transform rules as any shape; animated masks (referencing `time`) re-mask every frame.
* An empty `clip` block (mask only, no content) is legal and draws nothing.

---

## 12. Blend Modes (0.2)

```pvg
group
  pos [cx, cy]
  rot time * 1.5
  blend "add"        # "normal" (default) | "add" | "multiply" | "screen" | "overlay"

  circle
    center [0, 0]
    radius 30
    fill #ff8833
    blend "screen"   # Per-shape override
```

* **`"normal"`:** Standard source-over compositing (0.1 behavior).
* **`"add"` (additive):** Pixel values add and clamp — engine thrusters, laser trails, magic, pulsing shields. SVG export maps this to `mix-blend-mode: plus-lighter`.
* **`"multiply"`:** Vignettes, ambient occlusion, grit overlays.
* **`"screen"`:** Holographic UI, glass glare, specular highlights.
* **`"overlay"`:** Contrast-style energy effects.
* Blend applies at draw time against the current backdrop, in document order. Group-level `blend` becomes the inherited default for children unless a shape overrides it.

---

## 13. Groups & 2D Affine Transforms

A `group` bundles child nodes and applies a hierarchical $2 \times 3$ affine transform matrix:

```pvg
group
  pos [tx, ty]          # Translation offset
  rot angle             # Rotation angle (e.g. 45deg or 0.785rad)
  scale [sx, sy]        # Scaling factors
  opacity alpha         # Multiplicative opacity factor
  blend "add"           # 0.2: inherited blend default for children
  blur number           # 0.2: group-wide blur (see Section 10)
  shadow [dx, dy] r color  # 0.2: group-wide drop shadow
  glow r color          # 0.2: group-wide glow

  # Child elements inherit the combined world transform
  circle
    center [0, 0]
    radius 20
```

### 13.1 Matrix Composition
Local transforms compose into world coordinate space using matrix multiplication:

$$\begin{bmatrix} x' \\ y' \\ 1 \end{bmatrix} = \mathbf{M}_{\text{parent}} \times \begin{bmatrix} \cos\theta \cdot s_x & -\sin\theta \cdot s_y & t_x \\ \sin\theta \cdot s_x & \cos\theta \cdot s_y & t_y \\ 0 & 0 & 1 \end{bmatrix} \times \begin{bmatrix} x \\ y \\ 1 \end{bmatrix}$$

---

## 14. Time & Animation Model

PVG provides a native timeline clock variable `time` (and alias `t`) representing elapsed seconds ($t \in [0.0, \infty)$):

```pvg
PVG 0.1
canvas 600 600
  background #000000

set cx = 300
set cy = 300

# Continuous rotation at 2 radians per second
set angle = time * 2.0

# Procedural oscillation
set pulse = 20 + 10 * sin(time * 5.0)

circle
  center [cx + 150 * cos(angle), cy + 150 * sin(angle)]
  radius pulse
  fill #00ffcc
```

---

## 15. Memory Arena & Runtime Safety Limits

To guarantee safety when opening untrusted documents and to prevent denial-of-service (DoS) hangs:

| Parameter | Default Cap | Purpose |
| :--- | :--- | :--- |
| `MAX_LOOP_ITERATIONS` | $100,000$ | Prevents infinite `while true` execution hangs |
| `MAX_CALL_STACK_DEPTH`| $64$ frames | Prevents stack-overflow recursion crashes |
| `MAX_SCENE_PRIMITIVES`| $50,000$ | Limits maximum output draw commands |
| `WORKING_HEAP_BUDGET` | $< 50\text{ KB}$ | Bounded memory allocation for AST + Scene List |

---

## 16. Complete Reference Benchmark Presets

### Preset 1: Telemetry Dashboard Card (`presets/telemetry_card.pvg`)
```pvg
PVG 0.1
canvas 600 400
  background #0b0c10

# Card Background Container
rectangle
  pos [40, 40]
  size [520, 320]
  radius 12
  fill #12141c
  stroke #1f2333
  width 1.5

# Top Header Bar
rectangle
  pos [40, 40]
  size [520, 52]
  radius 12
  fill #181b26
  stroke none

# Card Header Title (Sans-Serif Font)
text
  pos [60, 56]
  content "TELEMETRY MONITOR"
  size 16
  font "sans"
  align "left"
  fill #00ffcc

# Live Status Beacon
circle
  center [480, 66]
  radius 4
  fill #00e676

text
  pos [495, 58]
  content "LIVE"
  size 13
  font "mono"
  align "left"
  fill #00e676

# Dynamic Animated Values
set rpm = floor(3200 + 400 * sin(time * 3.0))
set temp = floor(68 + 8 * cos(time * 2.0))

# Left Metric Card
rectangle
  pos [60, 115]
  size [225, 110]
  radius 8
  fill #171922
  stroke #282c3f
  width 1

text
  pos [80, 130]
  content "ENGINE SPEED"
  size 12
  font "sans"
  align "left"
  fill #8f96b0

text
  pos [80, 155]
  content "" + rpm + " RPM"
  size 28
  font "mono"
  align "left"
  fill #ffffff

# Right Metric Card
rectangle
  pos [315, 115]
  size [225, 110]
  radius 8
  fill #171922
  stroke #282c3f
  width 1

text
  pos [335, 130]
  content "CORE TEMP"
  size 12
  font "sans"
  align "left"
  fill #8f96b0

text
  pos [335, 155]
  content "" + temp + " °C"
  size 28
  font "mono"
  align "left"
  fill #ff3355

# Multi-alignment Footer
text
  pos [60, 310]
  content "LEFT: System OK"
  size 12
  font "mono"
  align "left"
  fill #5e6278

text
  pos [300, 310]
  content "CENTER: 60 FPS"
  size 12
  font "mono"
  align "center"
  fill #5e6278

text
  pos [540, 310]
  content "RIGHT: Time " + floor(time) + "s"
  size 12
  font "mono"
  align "right"
  fill #5e6278
```

### Preset 2: Radar Scanner (`presets/radar.pvg`)
```pvg
PVG 0.1
canvas 600 600
  background #080a0f

set cx = 300
set cy = 300
set sweep = time * 2.0

# Radar Concentric Range Rings
for r_idx from 1 to 4
  circle
    center [cx, cy]
    radius r_idx * 55
    fill none
    stroke #103b42
    width 1.5
    opacity 0.7

# Crosshair Lines
line
  from [cx - 240, cy]
  to   [cx + 240, cy]
  stroke #155560
  width 1
  opacity 0.5

line
  from [cx, cy - 240]
  to   [cx, cy + 240]
  stroke #155560
  width 1
  opacity 0.5

# Rotating Phosphor Sweep Trail
for trail from 0 to 20
  set a = sweep - trail * 0.035
  line
    from [cx, cy]
    to   [cx + 230 * cos(a), cy + 230 * sin(a)]
    stroke #00ffcc
    width 2
    opacity (1.0 - trail / 20) * 0.45

# Main Sweep Line
line
  from [cx, cy]
  to   [cx + 230 * cos(sweep), cy + 230 * sin(sweep)]
  stroke #ffffff
  width 2.5

# Orbiting Satellites with Pulsing Beacons
for b from 0 to 4
  set orbit_r = 65 + b * 40
  set speed = 0.6 + b * 0.25
  set b_angle = (time * speed) + b * 1.5
  set bx = cx + orbit_r * cos(b_angle)
  set by = cy + orbit_r * sin(b_angle)
  
  set pulse = 4 + 3 * sin(time * 8 + b * 2)
  circle
    center [bx, by]
    radius pulse
    fill #ff0055
    opacity 0.85
    stroke #ffffff
    width 1.5

  circle
    center [bx, by]
    radius pulse + 6
    fill none
    stroke #ff0055
    width 1
    opacity 0.35

# Central Hub Beacon
circle
  center [cx, cy]
  radius 8
  fill #00ffcc
  stroke #ffffff
  width 2
```

### Preset 3: Technical Dashboard Dial (`presets/dial.pvg`)
```pvg
PVG 0.1
canvas 600 600
  background #141419

set cx = 300
set cy = 300
set outer_r = 200
set inner_r = 170

# Outer Background Track
path
  stroke #2c2d35
  width 14
  fill none
  start [cx + outer_r * cos(135deg), cy + outer_r * sin(135deg)]
  arc [cx, cy] outer_r 135deg 405deg

# Colored Value Arc
path
  stroke #00d2ff
  width 14
  fill none
  start [cx + outer_r * cos(135deg), cy + outer_r * sin(135deg)]
  arc [cx, cy] outer_r 135deg 325deg

# Procedurally Generated Ticks
for i from 0 to 24
  set angle = 135deg + i * (270deg / 24)
  set is_major = (i % 4 == 0)
  set tick_len = is_major ? 18 : 8
  
  line
    from [cx + inner_r * cos(angle), cy + inner_r * sin(angle)]
    to   [cx + (inner_r - tick_len) * cos(angle), cy + (inner_r - tick_len) * sin(angle)]
    stroke is_major ? #ffffff : #666677
    width is_major ? 3 : 1
    opacity is_major ? 1.0 : 0.5

# Central Hub
circle
  center [cx, cy]
  radius 18
  fill #ffffff
  stroke #00d2ff
  width 4

# Gauge Pointer Needle
path
  fill #ff3355
  stroke none
   
  set needle_angle = 325deg
  set nx = cos(needle_angle)
  set ny = sin(needle_angle)
  set px = -ny * 7
  set py =  nx * 7
   
  start [cx + px, cy + py]
  line  [cx + nx * (inner_r - 25), cy + ny * (inner_r - 25)]
  line  [cx - px, cy - py]
  close
```

### Preset 4: Sci-Fi Energy Shield Core (`presets/shield_core.pvg`)

*The 0.2 reference asset: linear armor gradients, a clipped glass housing with hex texture and radial flare, a dashed tactical retainer ring with glow, additive rotating prisms with transparent-fade fills, and a screen-blended specular hub.*

```pvg
# PVG 0.2 reference preset: sci-fi energy shield core.
# Showcases gradients, clip, stroke topology, glow/shadow/blur, blend modes.
PVG 0.2
canvas 512 512
  background #07090e

set cx = 256
set cy = 256

# 1. Ambient Background Glow (Additive Lighting)
circle
  center [cx, cy]
  radius 160
  fill #00aaff
  blur 45
  blend "add"
  opacity 0.25

# 2. Outer Armor Plate with Drop Shadow & Metallic Linear Gradient
rectangle
  pos [106, 106]
  size [300, 300]
  radius 36
  fill linear [106, 106] [406, 406]
    stop 0.0 #2b3040
    stop 0.5 #171922
    stop 1.0 #0e1017
  stroke #48526e
  width 2
  join "miter"
  shadow [0, 18] 24 #000000bb

# 3. Clipped Core Window (Beveled Glass Housing)
clip
  circle
    center [cx, cy]
    radius 105
  for i from -5 to 5
    line
      from [cx - 150, cy + i * 30]
      to [cx + 150, cy + i * 30 + 60]
      stroke #00ffff
      width 1.5
      opacity 0.12
  circle
    center [cx, cy]
    radius 90
    fill radial [cx, cy] 90
      stop 0.0 #00ffff
      stop 0.6 #0033aa
      stop 1.0 #07090e

# 4. Mechanical Energy Retainer Ring (Exact Crisp Strokes)
circle
  center [cx, cy]
  radius 112
  fill none
  stroke #00d2ff
  width 4
  cap "butt"
  dash [28, 8, 12, 8]
  glow 8 #00d2ff80

# 5. Floating Energy Prisms (Rotated Group with Additive Blend)
group
  pos [cx, cy]
  rot time * 1.5
  blend "add"
  for p from 0 to 2
    set angle = p * (360deg / 3)
    path
      fill linear [0, 0] [cos(angle) * 70, sin(angle) * 70]
        stop 0.0 #ffffff
        stop 1.0 #0066ff00
      stroke #ffffff
      width 1
      join "miter"
      set r_inner = 35
      set r_outer = 68
      start [r_inner * cos(angle - 15deg), r_inner * sin(angle - 15deg)]
      line [r_outer * cos(angle), r_outer * sin(angle)]
      line [r_inner * cos(angle + 15deg), r_inner * sin(angle + 15deg)]
      close

# 6. Core Hub Highlight (Specularity)
circle
  center [cx - 6, cy - 6]
  radius 14
  fill #ffffff
  blur 4
  blend "screen"
```

### Preset 5: Tactical HUD (`presets/tactical_hud.pvg`)

*The post-0.2 reference asset: host-tunable `param` uniforms (shield/hull/temperature/callsign), a `carbon_mesh` pattern-filled chassis card with drop shadow, a 16×16 palette-indexed drone `sprite` from a `"""` block, a `noise2d`-deformed shield perimeter built by `for` + `if`/`else` inside `path`, and an array-driven telemetry `spline` — all in one 512×512 scene.*

```pvg
PVG 0.2
canvas 512 512
  background #06080d

param hull_hp: 0.65
param pilot_tag: "VIPER-7"

pattern carbon_mesh 16 16
  line
    from [0, 0] to [16, 16]
    stroke #ffffff0a
    width 1

rectangle
  pos [26, 26]
  size [460, 460]
  radius 24
  fill pattern carbon_mesh
  shadow [0, 16] 24 #000000ee

sprite
  pos [410, 48]
  scale 2
  palette [#00000000, #ff1a4b, #1e2333, #ffffff, #ffaa00]
  data """
  ..11........11..
  .1441......1441.
  ..11........11..
  """

path
  fill none
  stroke #00ffff
  width 2.5
  for deg from 0 to 360 step 6
    set rad = deg * (PI / 180)
    set n = noise2d(cos(rad) * 1.8 + time * 0.9, sin(rad) * 1.8 + time * 0.9)
    set pt = [256 + (100 + n * 12) * cos(rad), 230 + (100 + n * 12) * sin(rad)]
    if deg == 0
      start pt
    else
      line pt
  close

set wave = [12, 45, 68, 30, 85, 92, 40]
spline
  points wave
  pos [50, 395]
  size [412, 50]
  stroke #00ffcc
  width 2.5
```

---

## 17. Version History

### 0.1 — Core procedural vector engine
Geometric primitives (`circle`, `ellipse`, `rectangle`, `line`, `polygon`, `text`, `path` with 6 sub-commands), procedural control (`set`, `for`, `while`, `if`, `def`/`call`/`return`, `seed`), expressions with full operator precedence, `time`/`t` timeline clock, group affine transforms, flat solid-color styling, multi-backend draw list (GUI / SVG+SMIL / PNG / Android / Web).

### 0.2 — Game-asset rendering essentials (this document)
* **Stroke topology** (Section 8): `cap`, `join`, `miter`, `dash`, stroke `align`. Defaults reproduce 0.1 output exactly.
* **Paints & gradients** (Section 9): `linear` / `radial` (+focal) / `angular` (`conic` alias) with `stop` lists; transparent stops are meaningful fade-outs; new `Paint` runtime value.
* **Filters** (Section 10): `blur`, `shadow [dx, dy] r color`, `glow r color`, composited shadow → glow → shape with ~3r layer padding.
* **Clipping** (Section 11): `clip` blocks, first shape is the mask, intersection semantics, nestable.
* **Blending** (Section 12): `blend` on shapes, text, and groups (`normal`/`add`/`multiply`/`screen`/`overlay`).
* **Compatibility**: 0.2 readers accept `PVG 0.1` and `PVG 0.2` headers; all 0.1 documents evaluate identically. Fourteen words became reserved keywords (Section 2.7).
* **Reference preset**: `presets/shield_core.pvg` (Section 16, Preset 4).

### Post-0.2 engine batch (Section 18)
* **Host uniforms** (Section 18.1): `param name: default`, `Scene` host API.
* **Noise** (Section 18.2): `noise2d` / `noise3d`, angle helpers.
* **Pixel mode & sprites** (Section 18.3): `canvas snap` / `filter`, `sprite` blocks, `"""` multi-line strings (Section 2.4).
* **Patterns** (Section 18.4): top-level `pattern` tiles, `fill pattern name`, real tiling in every backend.
* **Arrays & splines** (Section 18.5): `Array` value type, `array` / `len` / `get`, `spline` statements.
* **Path control flow** (Section 18.6): `for` / `while` / `if` inside `path` bodies.
* **Soft keywords**: the new words stay usable as identifiers (Section 3 conventions).
* **Reference preset**: `presets/tactical_hud.pvg` (Section 16, Preset 5).

---

## 18. Host Uniforms, Noise, Sprites, Patterns & Splines (post-0.2 engine)

Implemented in the Rust core (`pvg` 0.2.x). Accepted under any `PVG 0.x` header. The new words (`param`, `pattern`, `sprite`, `spline`, `palette`, `data`, `row`, `snap`, `filter`) are *soft* keywords: they open new syntax in statement/property position but remain usable as variable, function, and loop-variable names elsewhere (so `for row from 0 to 7` in `grid.pvg` keeps working). The only exception is a bare `fill pattern` followed by a name, which always reads as a tile reference.

### 18.1 Host parameters (`param`)
```pvg
param health: 0.75        # `:` or `=` both accepted
param player_name: "Pilot_01"
rectangle
  pos [20, 20]
  size [200 * health, 12]
  fill health < 0.25 ? #ff3344 : #00e676
```
Declared defaults evaluate when the host does not override. Host API: `Scene::from_source(src)` → `scene.set_param("health", 0.2)` → `scene.evaluate()` per frame; or `compile_with_params(src, &[("health", Value::Number(0.2))], t)`, or `Evaluator::with_param` / `set_param`. `From<f64/i32/u32/bool/String/&str/Color/(f64,f64)/Vec<Value>/Vec<f64>>` is implemented for `Value`.

### 18.2 Deterministic noise
`noise2d(x, y)` / `noise3d(x, y, z)` → `[-1, 1]` integer-hash value noise (smoothstep-interpolated lattice, pure function of coordinates — no RNG state, identical across platforms). Also added: `radians()`, `degrees()`, `deg_to_rad()` fn, and `deg_to_rad` / `rad_to_deg` constants (`deg * deg_to_rad` works).

### 18.3 Pixel grid & sprites
```pvg
canvas 256 240
  snap 1.0           # round all coords to the grid (0 = off)
  filter "nearest"   # crisp pixels (SVG: crispEdges); default "linear"

sprite
  pos [m_x, m_y]
  palette [#00000000, #b84418, #fc9838]
  data "0110"        # `data` or `row`; chars 0-9/a-z = palette index, `.`/space = skip
  data "1221"
  data """           # ...or one triple-quoted block (raw, multi-line, dedented)
  0110
  1221
  """
```
`CanvasDecl` gains `snap: f64` + `pixel_filter`; `DrawList` carries both. `"""` strings are raw literals (no escapes); single-line `"""..."""` also works; blank rows are skipped for sprites.

### 18.4 Pattern fills
```pvg
pattern grid_pattern 16 16
  line
    from [0, 0]
    to [16, 0]
    stroke #ffffff22
    width 1

rectangle
  pos [40, 40]
  size [400, 300]
  fill pattern grid_pattern
```
Top-level `pattern name w h` + body; tiles pre-evaluated per frame (identity transform) into `DrawList.patterns`; `fill`/`stroke` accept `pattern name` (`Paint::Pattern`). SVG emits `<pattern id="pvg-pat-{name}">`; CPU rasterizers pre-render each tile once per frame and sample it per-pixel with Euclidean wrap (canvas-space aligned, origin `(0, 0)`; patterns referenced from inside a tile resolve to neutral gray, which terminates reference cycles).

### 18.5 Arrays & splines
Brackets with arity ≠ 2 are arrays (`[12, 45, 68]`); exactly-2 **scalar** brackets stay `Vec2` (back-compat); nested compounds (`[[10, 10], [90, 90]]`) are arrays; `array(a, b)` builds an explicit 2-element array. Builtins: `len(arr)`, `get(arr, i)` (negative wraps). `+` with a string coerces either side via display.
```pvg
set history = [12, 45, 68, 30, 85, 92, 40]
spline
  points history     # numbers auto-layout across pos/size, or array of [x, y]
  pos [20, 20]
  size [360, 120]
  stroke #00ffcc
  width 2
  glow 4 #00ffcc
```
Splines store control points (`DrawCmd::Spline`); all backends tessellate identically via `pvg::spline_to_bezier` (Catmull-Rom → cubic).

### 18.6 Control flow inside `path` (Section 7, EBNF updated)
`for` / `while` / `if` (with `else` / `else if` chains, nestable) may appear directly in `path` bodies, sharing the path's locals scope and loop safety budget — e.g. noise-deformed perimeters that `start` on the first iteration and `line` after. Style props (`fill`, `stroke`, …) must stay in the outer path body. Note: `? :` branches are *expressions*, so `cond ? start pt : line pt` is invalid — use `if`/`else`.

Reference presets: `presets/next_demo.pvg` (all five features), `presets/tactical_hud.pvg` (Section 16, Preset 5).
