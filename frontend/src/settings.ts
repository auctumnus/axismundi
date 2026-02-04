const hslToRgb = (
  h: number,
  s: number,
  l: number,
): [number, number, number] => {
  // normalize s and l to 0-1 range
  s /= 100;
  l /= 100;

  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;

  let r = 0,
    g = 0,
    b = 0;

  if (h >= 0 && h < 60) {
    r = c;
    g = x;
    b = 0;
  } else if (h >= 60 && h < 120) {
    r = x;
    g = c;
    b = 0;
  } else if (h >= 120 && h < 180) {
    r = 0;
    g = c;
    b = x;
  } else if (h >= 180 && h < 240) {
    r = 0;
    g = x;
    b = c;
  } else if (h >= 240 && h < 300) {
    r = x;
    g = 0;
    b = c;
  } else if (h >= 300 && h < 360) {
    r = c;
    g = 0;
    b = x;
  }

  // convert to 0-255 range
  const red = Math.round((r + m) * 255);
  const green = Math.round((g + m) * 255);
  const blue = Math.round((b + m) * 255);

  return [red, green, blue];
};

function hslToHex(h: number, s: number, l: number): string {
  const [red, green, blue] = hslToRgb(h, s, l);

  // convert to hex
  const toHex = (n: number) => n.toString(16).padStart(2, "0");

  return `#${toHex(red)}${toHex(green)}${toHex(blue)}`;
}

const hexToHsl = (hex: string): [number, number, number] => {
  // remove leading #
  hex = hex.replace(/^#/, "");

  // parse r, g, b
  const r = parseInt(hex.substring(0, 2), 16) / 255;
  const g = parseInt(hex.substring(2, 4), 16) / 255;
  const b = parseInt(hex.substring(4, 6), 16) / 255;

  if (isNaN(r) || isNaN(g) || isNaN(b)) {
    throw new Error("Invalid hex color");
  }

  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  let h = 0,
    s = 0,
    l = (max + min) / 2;

  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);

    switch (max) {
      case r:
        h = (g - b) / d + (g < b ? 6 : 0);
        break;
      case g:
        h = (b - r) / d + 2;
        break;
      case b:
        h = (r - g) / d + 4;
        break;
    }
  }

  h = Math.round(h * 60) % 360;
  s = Math.round(s * 100);
  l = Math.round(l * 100);

  return [h, s, l];
};

const darkBackgroundPage = [223, 0.48, 0.11] as const; //  hsl(223, 48%, 11%)
const lightBackgroundPage = [210, 0.4, 0.96] as const; // hsl(210, 40%, 96%)

const darkBgLuminance = hslToRelativeLuminance(...darkBackgroundPage);
const lightBgLuminance = hslToRelativeLuminance(...lightBackgroundPage);

function hslToRelativeLuminance(h: number, s: number, l: number): number {
  // h in [0, 360], s and l in [0, 1]

  // convert hsl to rgb
  let r: number, g: number, b: number;

  if (s === 0) {
    r = g = b = l;
  } else {
    const hueToRgb = (p: number, q: number, t: number): number => {
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1 / 6) return p + (q - p) * 6 * t;
      if (t < 1 / 2) return q;
      if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
      return p;
    };

    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    const hNorm = h / 360;

    r = hueToRgb(p, q, hNorm + 1 / 3);
    g = hueToRgb(p, q, hNorm);
    b = hueToRgb(p, q, hNorm - 1 / 3);
  }

  // linearize rgb values
  const linearize = (channel: number): number => {
    if (channel <= 0.04045) {
      return channel / 12.92;
    } else {
      return Math.pow((channel + 0.055) / 1.055, 2.4);
    }
  };

  const rLinear = linearize(r);
  const gLinear = linearize(g);
  const bLinear = linearize(b);

  // calculate relative luminance
  const luminance = 0.2126 * rLinear + 0.7152 * gLinear + 0.0722 * bLinear;

  return luminance;
}

const parseHSL = (color: string): [number, number, number] => {
  const hslRegex = /hsl\(\s*(\d{1,3})\s*,\s*(\d{1,3})%\s*,\s*(\d{1,3})%\s*\)/;
  const match = color.match(hslRegex);
  if (match) {
    const h = Number(match[1]);
    const s = Number(match[2]);
    const l = Number(match[3]);
    if (h >= 0 && h <= 360 && s >= 0 && s <= 100 && l >= 0 && l <= 100) {
      return [h, s, l];
    }
  }
  throw new Error("Invalid color format");
};

class ContrastResult {
  ratio: number;
  AA: {
    normal: boolean;
    large: boolean;
  };
  AAA: {
    normal: boolean;
    large: boolean;
  };

  constructor(ratio: number) {
    this.ratio = ratio;
    this.AA = {
      normal: ratio >= 4.5,
      large: ratio >= 3,
    };
    this.AAA = {
      normal: ratio >= 7,
      large: ratio >= 4.5,
    };
  }

  isAccessible(): boolean {
    return this.AA.large;
  }
}

const contrast = (background: string, color: string): ContrastResult => {
  const c = (bgLum: number, fgLum: number) => {
    const L1 = Math.max(bgLum, fgLum);
    const L2 = Math.min(bgLum, fgLum);
    const ratio = (L1 + 0.05) / (L2 + 0.05);
    return new ContrastResult(ratio);
  };

  const bgLum = background === "dark" ? darkBgLuminance : lightBgLuminance;

  const [h, s, l] = parseHSL(color);

  const fgLum = hslToRelativeLuminance(h, s / 100, l / 100);

  return c(bgLum, fgLum);
};

class GenderInput extends HTMLElement {
  private input: HTMLInputElement | null = null;
  private container: HTMLDivElement | null = null;
  private _internals: ElementInternals;

  private hue: number | null = null;
  private saturation: number | null = null;
  private lightness: number | null = null;

  private preview: HTMLDivElement | null = null;

  private hueSlider: HTMLInputElement | null = null;
  private saturationSlider: HTMLInputElement | null = null;
  private lightnessSlider: HTMLInputElement | null = null;

  private saturationValue: HTMLInputElement | null = null;
  private lightnessValue: HTMLInputElement | null = null;
  private hueValue: HTMLInputElement | null = null;

  private textPreview: HTMLDivElement | null = null;
  private warning: HTMLDivElement | null = null;

  constructor() {
    super();
    this._internals = this.attachInternals();
    const initialValue = this.getAttribute("initial-value");
    if (initialValue) {
      try {
        const [h, s, l] = hexToHsl(initialValue);
        this.hue = h;
        this.saturation = s;
        this.lightness = l;
      } catch {
        this.hue = null;
        this.saturation = null;
        this.lightness = null;
      }
    }
    this.render();
  }

  private contrastWarningText(): string {
    const contrast = this.contrast();
    if (!contrast) return "";
    if (contrast.light.isAccessible() && contrast.dark.isAccessible()) {
      return "";
    } else if (
      !contrast.light.isAccessible() &&
      !contrast.dark.isAccessible()
    ) {
      return "the selected color does not have sufficient contrast against either light or dark backgrounds";
    } else if (!contrast.light.isAccessible()) {
      return "the selected color does not have sufficient contrast against light backgrounds";
    } else if (!contrast.dark.isAccessible()) {
      return "the selected color does not have sufficient contrast against dark backgrounds";
    }
    return "";
  }

  render() {
    const pfp = document.querySelector(
      "#current-user",
    ) as HTMLImageElement | null;
    const contrastWarning = this.contrastWarningText();

    this.innerHTML = `
    <div style="--hue: ${this.hue ?? 180}; --saturation: ${this.saturation ?? 50}%; --lightness: ${this.lightness ?? 50}%" id="gender-container">
        <div style="--color: ${this.value};" id="color-preview">
            ${this.value ? "" : "<span>No color selected</span>"}
        </div>
        <div class="controls">
            <div class="sliders">
                <div class="slider-group">
                    <div class="slider-container">
                        <input type="range" aria-label="Hue" id="hue-slider" min="0" max="359" value="${this.hue || "0"}">
                        <input type="text" id="hue-value" value="${this.hue ?? ""}">
                    </div>
                </div>
                
                <div class="slider-group">
                    <div class="slider-container">
                        <input type="range" aria-label="Saturation" id="saturation-slider" min="0" max="100" value="${this.saturation || "50"}">
                        <input type="text" id="saturation-value" value="${this.saturation ?? ""}">
                    </div>
                </div>
                
                <div class="slider-group">
                    <div class="slider-container">
                        <input type="range" aria-label="Lightness" id="lightness-slider" min="0" max="100" value="${this.lightness || "50"}">
                        <input type="text" id="lightness-value" value="${this.lightness ?? ""}">
                    </div>
                </div>
            </div>
            <section>
                <label for="gender">Hex value</label>
                <input type="text" id="gender" name="gender" value="${this.value}">
                <button class="clear" id="clear-button">Clear</button>
            </section>
        </div>
    </div>
    <div id="gender-text-preview" class="text-preview" ${this.value ? `style="--color: ${this.value};"` : ""}>
        <div class="text-preview-light">
            <img class="pfp medium" src="${pfp ? pfp.src : "/static/default_pfp.png"}" alt="profile picture">
            <span class="display-name ${this.value ? "gendered" : ""}">${window.user.display_name || window.user.username}</span>
        </div>
        <div class="text-preview-dark">
            <img class="pfp medium" src="${pfp ? pfp.src : "/static/default_pfp.png"}" alt="profile picture">
            <span class="display-name ${this.value ? "gendered" : ""}">${window.user.display_name || window.user.username}</span>
        </div>
    </div>
    <div class="warning" id="gender-warning" hidden=${contrastWarning ? "false" : "true"}>
        <h2>warning</h2>
        <p>${contrastWarning}</p>
    </div>
    `;
    // ^ `boolean | undefined` is kind of hilarious
    this.attachElements();
    this.attachCallbacks();
  }

  private contrast(): { light: ContrastResult; dark: ContrastResult } | null {
    if (!this.value) return null;
    const light = contrast(
      "light",
      `hsl(${this.hue}, ${this.saturation}%, ${this.lightness}%)`,
    );
    const dark = contrast(
      "dark",
      `hsl(${this.hue}, ${this.saturation}%, ${this.lightness}%)`,
    );
    return {
      light,
      dark,
    };
  }

  private attachElements() {
    this.preview = document.getElementById("color-preview") as HTMLDivElement;
    this.hueSlider = document.getElementById("hue-slider") as HTMLInputElement;
    this.saturationSlider = document.getElementById(
      "saturation-slider",
    ) as HTMLInputElement;
    this.lightnessSlider = document.getElementById(
      "lightness-slider",
    ) as HTMLInputElement;

    this.hueValue = document.getElementById("hue-value") as HTMLInputElement;
    this.saturationValue = document.getElementById(
      "saturation-value",
    ) as HTMLInputElement;
    this.lightnessValue = document.getElementById(
      "lightness-value",
    ) as HTMLInputElement;

    this.input = this.querySelector("#gender") as HTMLInputElement;
    this.container = document.getElementById(
      "gender-container",
    ) as HTMLDivElement;

    this.textPreview = document.getElementById(
      "gender-text-preview",
    ) as HTMLDivElement;
    this.warning = document.getElementById("gender-warning") as HTMLDivElement;
  }

  private setDefaults() {
    if (this.hue === null) {
      this.hue = 0;
      this.hueSlider!.value = "0";
      this.hueValue!.value = "0";
    }
    if (this.saturation === null) {
      this.saturation = 50;
      this.saturationSlider!.value = "50";
      this.saturationValue!.value = "50";
    }
    if (this.lightness === null) {
      this.lightness = 50;
      this.lightnessSlider!.value = "50";
      this.lightnessValue!.value = "50";
    }
  }

  private updatePreview() {
    if (
      Number.isNaN(this.hue) ||
      Number.isNaN(this.saturation) ||
      Number.isNaN(this.lightness)
    ) {
      this.hue = null;
      this.saturation = null;
      this.lightness = null;
    }
    if (this.preview) {
      this.preview.style.setProperty("--color", this.value);
      this.preview.innerHTML = this.value
        ? ""
        : "<span>No color selected</span>";
    }
    if (this.input) {
      this.input.value = this.value;
    }
    if (this.container) {
      this.container.style.setProperty("--hue", this.hue + "");
      this.container.style.setProperty("--saturation", this.saturation + "%");
      this.container.style.setProperty("--lightness", this.lightness + "%");
    }
    if (this.textPreview) {
      if (this.value) {
        this.textPreview.style.setProperty("--color", this.value);
        for (const span of [...this.textPreview.querySelectorAll("span")]) {
          span.classList.add("gendered");
        }
      } else {
        this.textPreview.style.removeProperty("--color");
        for (const span of [...this.textPreview.querySelectorAll("span")]) {
          span.classList.remove("gendered");
        }
      }
    }
    if (this.warning) {
      const contrastWarning = this.contrastWarningText();
      if (contrastWarning) {
        this.warning.hidden = false;
      } else {
        this.warning.hidden = true;
      }
      this.warning.querySelector("p")!.textContent = contrastWarning;
    }
  }

  private clear() {
    this.hue = null;
    this.saturation = null;
    this.lightness = null;
    this.render();
  }

  private attachCallbacks() {
    const kinds = ["hue", "saturation", "lightness"] as const;

    for (const kind of kinds) {
      const slider = this[`${kind}Slider`] as HTMLInputElement;
      const valueInput = this[`${kind}Value`] as HTMLInputElement;
      slider?.addEventListener("input", (e) => {
        const target = e.target as HTMLInputElement;
        const val = Number(target.value);
        this.setDefaults();
        (this as any)[kind] = val;
        valueInput.value = target.value;
        this.updatePreview();
      });
      valueInput?.addEventListener("change", (e) => {
        const target = e.target as HTMLInputElement;
        let val = Number(target.value);
        this.setDefaults();
        const min = Number(slider.min);
        const max = Number(slider.max);
        if (val < min) val = min;
        if (val > max) val = max;
        (this as any)[kind] = val;
        slider.value = String(val);
        target.value = String(val);
        this.updatePreview();
      });
    }

    this.input?.addEventListener("change", (e) => {
      const target = e.target as HTMLInputElement;
      const color = target.value;
      try {
        const [h, s, l] = hexToHsl(color);
        this.hue = h;
        this.saturation = s;
        this.lightness = l;

        this.hueSlider!.value = String(h);
        this.saturationSlider!.value = String(s);
        this.lightnessSlider!.value = String(l);
        this.hueValue!.value = String(h);
        this.saturationValue!.value = String(s);
        this.lightnessValue!.value = String(l);

        this.updatePreview();
      } catch {
        this.hue = null;
        this.saturation = null;
        this.lightness = null;
        this.updatePreview();
      }
    });

    const clearButton = this.querySelector(
      "#clear-button",
    ) as HTMLButtonElement;
    clearButton.addEventListener("click", (e) => {
      e.preventDefault();
      this.clear();
    });
  }

  connectedCallback() {
    this.render();
  }

  get value(): string {
    if (
      this.hue === null ||
      this.saturation === null ||
      this.lightness === null
    ) {
      return "";
    }
    return hslToHex(this.hue, this.saturation, this.lightness);
  }
}

customElements.define("gender-input", GenderInput);
