// One probe, one object: everything a page can be judged on without a pair of eyes.
//
// Evaluate this expression in a real browser against a deployed page and the result is the finding
// set: sideways overflow and the elements behind it, every text element below its contrast minimum
// (with the composited background it is really drawn on), overlapping boxes, clickable things with no
// accessible name, tap targets under 44px, a page that rendered nothing, a title that no tab can show,
// and the heading/language/alt structure.
//
// Plain expression, no imports, no build. Drive it from any scriptable browser: for each theme and
// width, set the theme, set the viewport, navigate, evaluate, and store the result under
// "<theme>/<width><path>" so two passes are comparable. Re-apply the viewport every ~6 navigations
// (an expired override silently measures the wrong width) and check the clientWidth values you got
// are the ones you asked for.
//
// Before trusting a clean sweep, plant a canary and require the probe to catch it:
//   const el = document.createElement('span');
//   el.textContent = 'canary';
//   el.style.cssText = 'color:#fefefe;background:#f6f3ec;font-size:16px;padding:2px';
//   document.body.appendChild(el);
//
// Then re-plant the real defect the metric was written for: inject `main{padding-inline:0 !important}` and
// require `gutter.min` to fall to 0 and return when the style is removed.
(() => {
  const de = document.documentElement;
  const vw = de.clientWidth;
  const vh = de.clientHeight;
  const out = {
    path: location.pathname + location.search,
    theme: de.dataset.theme || "(none)",
    vw,
    vh,
    title: document.title,
    titleChars: document.title.length,
    titleRepeatsSite: /([\w.-]{2,})[^\w]{1,6}\1\s*$/.test(document.title.trim()),
  };

  // --- sideways scroll -------------------------------------------------------
  out.overflowX = de.scrollWidth - vw;

  const inScrollBox = (el) => {
    let n = el;
    while (n && n !== document.body) {
      const ox = getComputedStyle(n).overflowX;
      if (ox === "auto" || ox === "scroll") return true;
      n = n.parentElement;
    }
    return false;
  };
  const wide = [...document.querySelectorAll("body *")].filter((el) => {
    const r = el.getBoundingClientRect();
    return r.width > vw + 1 && r.right > vw + 1;
  });
  const wideUnscrollable = wide.filter((el) => !inScrollBox(el));
  out.wide = {
    count: wideUnscrollable.length,
    sample: wideUnscrollable.slice(0, 5).map((el) => ({
      tag: el.tagName.toLowerCase(),
      cls: String(el.className || "").slice(0, 40),
      w: Math.round(el.getBoundingClientRect().width),
    })),
  };

  // --- colour ----------------------------------------------------------------
  const parse = (c) => {
    const m = c && c.match(/rgba?\(([^)]+)\)/);
    if (!m) return null;
    const p = m[1].split(",").map(Number);
    return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 };
  };
  const lum = ({ r, g, b }) => {
    const f = (v) => {
      v /= 255;
      return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
    };
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
  };
  // Translucent layers are composited, never skipped: an 85%-opaque badge is neither its own colour
  // nor the card's, and skipping it measures the text against the wrong background entirely.
  const paintedBg = (el) => {
    const layers = [];
    let n = el;
    while (n && n !== de) {
      const c = parse(getComputedStyle(n).backgroundColor);
      if (c && c.a > 0.002) {
        layers.push(c);
        if (c.a > 0.999) break;
      }
      n = n.parentElement;
    }
    let out = parse(getComputedStyle(document.body).backgroundColor) || { r: 0, g: 0, b: 0, a: 1 };
    for (let i = layers.length - 1; i >= 0; i--) {
      const l = layers[i];
      out = {
        r: l.r * l.a + out.r * (1 - l.a),
        g: l.g * l.a + out.g * (1 - l.a),
        b: l.b * l.a + out.b * (1 - l.a),
        a: 1,
      };
    }
    return out;
  };
  const hidden = (el) => {
    if (el.closest(".visually-hidden, .sr-only, [hidden], [aria-hidden='true']")) return true;
    const cs = getComputedStyle(el);
    if (cs.visibility === "hidden" || cs.display === "none" || cs.opacity === "0") return true;
    const r = el.getBoundingClientRect();
    return r.width < 1 || r.height < 1;
  };

  const leaves = [...document.querySelectorAll("body *")]
    .filter((el) => {
      const own = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
      if (!own) return false;
      return !hidden(el);
    })
    .slice(0, 600);

  const violations = [];
  for (const el of leaves) {
    const cs = getComputedStyle(el);
    const size = parseFloat(cs.fontSize);
    const bold = parseInt(cs.fontWeight, 10) >= 700;
    const large = size >= 24 || (bold && size >= 18.66);
    const want = large ? 3 : 4.5;
    const fg = parse(cs.color);
    if (!fg) continue;
    const bg = paintedBg(el);
    const l1 = lum(fg);
    const l2 = lum(bg);
    const [hi, lo] = l1 > l2 ? [l1, l2] : [l2, l1];
    const ratio = Math.round(((hi + 0.05) / (lo + 0.05)) * 100) / 100;
    if (ratio < want - 0.1) {
      violations.push({
        ratio,
        want,
        tag: el.tagName.toLowerCase(),
        cls: String(el.className || "").slice(0, 30),
        text: (el.textContent || "").trim().replace(/\s+/g, " ").slice(0, 40),
        colour: cs.color,
        on: `rgb(${Math.round(bg.r)}, ${Math.round(bg.g)}, ${Math.round(bg.b)})`,
      });
    }
  }
  violations.sort((a, b) => a.ratio - b.ratio);
  out.contrast = { checked: leaves.length, count: violations.length, worst: violations.slice(0, 8) };

  // --- things on top of each other -------------------------------------------
  // Inline boxes are excluded: their rect is the union of their line boxes, so a wrapped link overlaps
  // its neighbours by construction and every "finding" is the instrument.
  const rects = leaves
    .filter((el) => getComputedStyle(el).display !== "inline")
    .map((el) => ({ el, r: el.getBoundingClientRect() }))
    .filter(({ r }) => r.width > 4 && r.height > 4 && r.top < vh && r.bottom > 0)
    .slice(0, 220);
  const overlaps = [];
  for (let i = 0; i < rects.length && overlaps.length < 6; i++) {
    for (let j = i + 1; j < rects.length; j++) {
      const a = rects[i];
      const b = rects[j];
      if (a.el.contains(b.el) || b.el.contains(a.el)) continue;
      const dx = Math.min(a.r.right, b.r.right) - Math.max(a.r.left, b.r.left);
      const dy = Math.min(a.r.bottom, b.r.bottom) - Math.max(a.r.top, b.r.top);
      if (dx > 6 && dy > 6) {
        overlaps.push({
          a: a.el.tagName.toLowerCase() + "." + String(a.el.className || "").slice(0, 20),
          b: b.el.tagName.toLowerCase() + "." + String(b.el.className || "").slice(0, 20),
          dx: Math.round(dx),
          dy: Math.round(dy),
        });
        break;
      }
    }
  }
  out.overlaps = overlaps;

  // --- clickable but unlabelled, and too small to hit ------------------------
  const interactive = [...document.querySelectorAll("a[href], button, [role=button]")].filter((el) => !hidden(el));
  out.unlabelled = interactive
    .filter((el) => {
      const text = (el.textContent || "").trim();
      return !text && !el.getAttribute("aria-label") && !el.getAttribute("title") && !el.querySelector("img[alt]:not([alt=''])");
    })
    .map((el) => el.tagName.toLowerCase() + "." + String(el.className || "").slice(0, 30))
    .slice(0, 6);
  out.smallTargets = interactive
    .map((el) => {
      const r = el.getBoundingClientRect();
      return { label: (el.textContent || el.getAttribute("aria-label") || el.tagName).trim().slice(0, 20), w: Math.round(r.width), h: Math.round(r.height) };
    })
    .filter((t) => t.w > 0 && t.h > 0 && (t.w < 44 || t.h < 44))
    .slice(0, 8);

  // --- a page that rendered nothing -----------------------------------------
  const main = document.querySelector("main") || document.body;
  out.mainChars = main.innerText.trim().length;
  out.mainText = main.innerText.trim().replace(/\s+/g, " ").slice(0, 160);

  // --- the page frame --------------------------------------------------------
  // Where the content starts: the half of layout that overflow and contrast cannot see. `0` is a finding,
  // and so is a spread — blocks at 0 and at 8 on one page means each component padded itself and most did
  // not. A site where every page began at x=0 passed a sweep clean and was spotted by a person in a glance.
  const blocks = [...main.querySelectorAll("h1, p, table, ul, ol, form, nav, section, article, dl, figure")].filter(
    (el) => {
      const r = el.getBoundingClientRect();
      return r.width > 8 && r.height > 8 && (el.innerText || "").trim().length > 0;
    },
  );
  const lefts = blocks.map((el) => Math.round(el.getBoundingClientRect().left));
  out.gutter = {
    mainPad: getComputedStyle(main).paddingLeft,
    min: lefts.length ? Math.min(...lefts) : null,
    distinct: [...new Set(lefts)].sort((a, b) => a - b).slice(0, 5),
  };

  // --- headings, language, alt text -----------------------------------------
  const heads = [...document.querySelectorAll("h1, h2, h3, h4, h5, h6")].map((h) => Number(h.tagName[1]));
  let skips = 0;
  for (let i = 1; i < heads.length; i++) if (heads[i] - heads[i - 1] > 1) skips++;
  const imgs = [...document.images];
  out.structure = {
    lang: de.getAttribute("lang"),
    h1: heads.filter((h) => h === 1).length,
    headingSkips: skips,
    headings: heads.slice(0, 12),
    images: imgs.length,
    imagesWithoutAlt: imgs.filter((i) => !i.hasAttribute("alt")).length,
    brokenImages: imgs.filter((i) => i.complete && i.naturalWidth === 0).length,
    skipLink: !!document.querySelector("a[href^='#']:not([href='#'])"),
  };

  return out;
})()
