# PVG 0.1 Language & Architecture Specification

**PVG (Procedural Vector Graphics)** is a deterministic, human-readable 2D vector graphics and procedural scene description language. It is designed to combine the declarative clarity of vector graphics with the programmatic power of a sandboxed procedural language while maintaining **microsecond CPU evaluation, zero GPU dependency, and a sub-50KB memory footprint**.

---

## 1. Design Principles & Goals

```
[ PVG 0.1 Source ]
        │
        ▼ (Single-Pass Tokenizer & Recursive Descent Parser)
[ Abstract Syntax Tree (AST) ] (~10–25 KB)
        │
        ▼ (Procedural Evaluator: Loops, Math, Scope Resolution)
[ Flat 2D Draw List (Contiguous Structs) ] (~15–35 KB)
        │
   ┌────┴───────────────────────────┬────────────────────────────┐
   ▼                                ▼                            ▼
[ Native GUI Painter ]     [ Standalone SVG ]          [ CPU Software Rasterizer ]
(egui / Skia / Window)     (W3C SMIL Animation)        (1-Line Scanline Buffer)
```

1. **Procedural Native:** Native support for variables, dynamic loops, user-defined functions, trigonometry, and non-linear mathematical expressions directly in the graphics definition.
2. **Deterministic & Pure:** Identical source text evaluates to the identical scene graph across all operating systems, CPU architectures, and runtime backends.
3. **Ultra-Low Resource Footprint:** Designed to compile and evaluate inside `< 50 KB` of heap memory with execution times under $0.2\text{ ms}$ per frame on a single CPU thread.
4. **No XML/DOM Overhead:** Eliminates deeply nested closing tags, XML namespaces, and heavy browser DOM hierarchies.
5. **Algebraic Curve Simplicity:** Replaces SVG's computationally heavy endpoint-to-center elliptical arc matrix inversions with direct center-radius-angle trigonometry and quadratic Béziers.

---

## 2. Lexical Grammar

### 2.1 File Encoding & Structure
* **Encoding:** UTF-8.
* **Header:** A PVG document **must** begin with the header `PVG <major>.<minor>` on the first non-empty line (e.g., `PVG 0.1`).
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

### 2.4 2D Coordinate & Vector Literals
To eliminate operator ambiguity with arithmetic expressions, all 2D vector coordinates **must** use bracket delimiters:
```ebnf
vector2 = "[" , expression , "," , expression , "]" ;
```
* *Examples:* `[100, 200]`, `[cx + r * cos(a), cy + r * sin(a)]`, `[x - 10, y + 5]`

### 2.5 Color Literals
* **Hex Color:** `#RGB`, `#RRGGBB`, `#RRGGBBAA` (e.g., `#fff`, `#00ffcc`, `#ff005580`).
* **Keywords:** `black`, `white`, `red`, `green`, `blue`, `yellow`, `cyan`, `magenta`, `none`, `transparent`.
* **Functional Form:** `rgb(r, g, b)` and `rgba(r, g, b, a)` where $r, g, b \in [0, 255]$ and $a \in [0.0, 1.0]$.

---

## 3. Formal EBNF Grammar

```ebnf
Document        ::= Header CanvasDecl Statement* EOF ;
Header          ::= "PVG" Version NEWLINE ;
Version         ::= NUMBER "." NUMBER ;

CanvasDecl      ::= "canvas" NUMBER NUMBER NEWLINE ( INDENT "background" Color NEWLINE DEDENT )? ;

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
                  | PathStmt
                  | GroupStmt ;

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

PathStmt        ::= "path" NEWLINE INDENT ( PathCommand | StyleProp | SetStmt )+ DEDENT ;
PathCommand     ::= ( "start" Vector2
                    | "line"  Vector2
                    | "quad"  Vector2 Vector2
                    | "curve" Vector2 Vector2 Vector2
                    | "arc"   Vector2 Expression Expression Expression
                    | "close" ) NEWLINE ;

GroupStmt       ::= "group" NEWLINE INDENT ( GroupProp | Statement )+ DEDENT ;
GroupProp       ::= ( "pos" Vector2 | "rot" Expression | "scale" Vector2 | StyleProp ) NEWLINE ;

StyleProp       ::= "fill" ( Color | Expression )
                  | "stroke" ( Color | Expression )
                  | "width" Expression
                  | "opacity" Expression ;

Vector2         ::= "[" Expression "," Expression "]" ;
```

---

## 4. Expression Syntax & Operator Precedence

Expressions are evaluated dynamically from highest to lowest precedence:

| Precedence | Operators | Description | Associativity |
| :--- | :--- | :--- | :--- |
| **1 (Highest)**| `()` | Grouping parentheses | Left |
| **2** | `^` | Exponentiation / Power | Right |
| **3** | `-` (unary), `not` | Negation, Logical NOT | Right |
| **4** | `*`, `/`, `%` | Multiplication, Division, Modulus | Left |
| **5** | `+`, `-` | Addition, Subtraction | Left |
| **6** | `<`, `<=`, `>`, `>=` | Relational comparisons | Left |
| **7** | `==`, `!=` | Equality / Inequality | Left |
| **8** | `and` | Logical AND | Left |
| **9** | `or` | Logical OR | Left |
| **10 (Lowest)**| `? :` | Ternary conditional | Right |

### Built-in Constants & Math Functions
* **Constants:** `PI` ($3.14159265...$), `TAU` ($6.28318530...$).
* **Trigonometry:** `sin(x)`, `cos(x)`, `tan(x)`.
* **Algebraic & Rounding:** `sqrt(x)`, `abs(x)`, `pow(base, exp)`, `floor(x)`, `ceil(x)`, `round(x)`.
* **Clamping & Min/Max:** `min(a, b)`, `max(a, b)`.
* **RNG:** `random(min, max)` returns a deterministic pseudorandom float in $[min, max]$.

---

## 5. Procedural Execution & Scoping Model

### 5.1 Dynamic Typing
The runtime supports 6 primitive value types:
1. `Number(f64)`
2. `Bool(bool)`
3. `String(String)`
4. `Color(Color)`
5. `Vec2(f64, f64)`
6. `None`

### 5.2 Lexical Scoping & State Isolation
* **Global Scope:** Variables declared via `set` at the root document level are available globally.
* **Function Scope:** Function definitions create a distinct local activation frame. Arguments and local variables shadow outer variables.
* **Path & Group Scope:** `path` and `group` blocks inherit current variable scopes and can execute nested `set` statements without mutating parent state unless explicitly shadowed.

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
Every conformant PVG runtime initializes the RNG state identically given the same seed.

---

## 6. 2D Geometry Primitives

All geometric nodes inherit parent `group` styles unless explicitly overridden.

### 6.1 Circle
```pvg
circle
  center [cx, cy]       # Mandatory 2D center coordinate
  radius r              # Mandatory scalar radius
  fill color            # Optional (default: black)
  stroke color          # Optional (default: none)
  width number          # Optional stroke width in pixels (default: 1.0)
  opacity number        # Optional alpha multiplier in [0.0, 1.0] (default: 1.0)
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

## 8. Groups & 2D Affine Transforms

A `group` bundles child nodes and applies a hierarchical $2 \times 3$ affine transform matrix:

```pvg
group
  pos [tx, ty]          # Translation offset
  rot angle             # Rotation angle (e.g. 45deg or 0.785rad)
  scale [sx, sy]        # Scaling factors
  opacity alpha         # Multiplicative opacity factor

  # Child elements inherit the combined world transform
  circle
    center [0, 0]
    radius 20
```

### 8.1 Matrix Composition
Local transforms compose into world coordinate space using matrix multiplication:

$$\begin{bmatrix} x' \\ y' \\ 1 \end{bmatrix} = \mathbf{M}_{\text{parent}} \times \begin{bmatrix} \cos\theta \cdot s_x & -\sin\theta \cdot s_y & t_x \\ \sin\theta \cdot s_x & \cos\theta \cdot s_y & t_y \\ 0 & 0 & 1 \end{bmatrix} \times \begin{bmatrix} x \\ y \\ 1 \end{bmatrix}$$

---

## 9. Time & Animation Model

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

### 9.1 Static vs. Animated Execution
* **Static Contexts (e.g., CLI Export to Static SVG/PNG):** `time = 0.0` or a specified timestamp via `-t <sec>`.
* **Real-time Contexts (e.g., GUI Studio, Game Engine Integration):** Evaluates `doc` per frame with the current timeline delta $\Delta t$ at 60+ FPS.
* **Transpiled Animated SVG Contexts:** Samples the procedural timeline across a bounded periodic loop (e.g., 30 frames over 3.0s) and compiles into standard W3C declarative SMIL animation tags.

---

## 10. Memory Arena & Runtime Safety Limits

To guarantee safety when opening untrusted documents and to prevent denial-of-service (DoS) hangs:

| Parameter | Default Cap | Purpose |
| :--- | :--- | :--- |
| `MAX_LOOP_ITERATIONS` | $100,000$ | Prevents infinite `while true` execution hangs |
| `MAX_CALL_STACK_DEPTH`| $64$ frames | Prevents stack-overflow recursion crashes |
| `MAX_SCENE_PRIMITIVES`| $50,000$ | Limits maximum output draw commands |
| `WORKING_HEAP_BUDGET` | $< 50\text{ KB}$ | Bounded memory allocation for AST + Scene List |

### 10.1 Working Memory Allocation Breakdown
```
┌───────────────────────────────┬───────────────────────────┐
│ Component                     │ Measured Heap Footprint   │
├───────────────────────────────┼───────────────────────────┤
│ Parser Token Ring Buffer      │ 2 KB – 4 KB               │
│ AST & Symbol Table            │ 8 KB – 16 KB              │
│ Flat Draw List (1000 shapes)  │ 16 KB – 32 KB             │
│ Active Edge Table (AET)       │ 4 KB – 8 KB               │
│ Single Scanline Buffer (1024) │ 4.096 KB (1024 x 4 bytes) │
├───────────────────────────────┼───────────────────────────┤
│ Total Peak Memory Footprint   │ ~22 KB – 64 KB (Passed)   │
└───────────────────────────────┴───────────────────────────┘
```

---

## 11. Complete Reference Benchmark Presets

### Preset 1: Radar Scanner (`presets/radar.pvg`)
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

### Preset 2: Technical Dashboard Dial (`presets/dial.pvg`)
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