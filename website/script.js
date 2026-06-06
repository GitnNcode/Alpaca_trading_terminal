/* =====================================================================
   Alpaca Trading Terminal — download site behavior (vanilla JS, no deps)
   - Detect the visitor's OS from the browser
   - Point the hero "Download" button at the matching binary
   - Highlight the recommended download card + show its badge
   - Fall back gracefully when the OS is unknown or mobile
   - Reveal-on-scroll animations + copy-to-clipboard for code blocks

   NOTE ON FILENAMES: this script never hardcodes a binary filename. It reads
   the href / data-file / data-size straight off the download cards in
   index.html. To change a build, edit the card markup — nothing here.
   ===================================================================== */
(function () {
  'use strict';

  var OS_LABELS = {
    windows: 'Windows',
    macos: 'macOS',
    linux: 'Linux',
    android: 'Android',
    ios: 'iOS',
    unknown: 'your system'
  };

  /* ---- 1. Detect the operating system ------------------------------ */
  function detectOS() {
    var uaData = navigator.userAgentData;
    var platform = (uaData && uaData.platform) ? uaData.platform : (navigator.platform || '');
    var ua = navigator.userAgent || '';
    var p = (platform + ' ' + ua).toLowerCase();

    // Mobile first — there is no mobile build, and Android UA also says "linux".
    if (/android/.test(p)) return 'android';
    if (/iphone|ipad|ipod/.test(p)) return 'ios';
    // iPadOS 13+ masquerades as a Mac; a touch-capable "Mac" is really an iPad.
    if (/mac/.test(p) && navigator.maxTouchPoints > 1) return 'ios';

    if (/win/.test(p)) return 'windows';
    if (/mac|darwin/.test(p)) return 'macos';
    if (/linux|x11|ubuntu|fedora|debian|cros/.test(p)) return 'linux';
    return 'unknown';
  }

  /* ---- 2. Wire the detected OS into the page ----------------------- */
  function applyOS(os) {
    var hero = document.getElementById('heroDownload');
    var heroLabel = document.getElementById('heroDownloadLabel');
    var status = document.getElementById('osStatus');
    var fallback = document.getElementById('osFallback');
    var card = document.getElementById('card-' + os); // only for win/mac/linux

    function setHero(label, href, isFile) {
      if (heroLabel) heroLabel.textContent = label;
      if (!hero) return;
      hero.setAttribute('href', href);
      if (isFile) { hero.setAttribute('download', ''); }
      else { hero.removeAttribute('download'); }
    }

    function highlight(targetCard) {
      if (!targetCard) return;
      targetCard.classList.add('is-recommended');
      var badge = targetCard.querySelector('.recommended-badge');
      if (badge) badge.classList.remove('hidden');
    }

    if (os === 'windows' || os === 'macos' || os === 'linux') {
      var primary = card ? card.querySelector('[data-primary]') : null;
      if (primary) {
        var file = primary.getAttribute('data-file') || '';
        var size = primary.getAttribute('data-size') || '';
        setHero('Download for ' + OS_LABELS[os], primary.getAttribute('href'), true);
        if (hero) hero.setAttribute('title', file);
        if (status) status.textContent = '> detected ' + OS_LABELS[os] + ' — ' + file + (size ? ' (' + size + ')' : '');
        highlight(card);
      } else {
        // No primary download on this card — point at the download section.
        setHero('See downloads', '#download', false);
        highlight(card);
      }
    } else if (os === 'android' || os === 'ios') {
      // Desktop-only app.
      if (status) status.textContent = '> ' + OS_LABELS[os] + ' detected — this is a desktop app for macOS or Windows';
      if (fallback) {
        fallback.textContent = 'This is a desktop app — open this page on macOS or Windows to download a binary, or build from source for Linux.';
        fallback.classList.remove('hidden');
      }
      setHero('See downloads', '#download', false);
    } else {
      // Unknown — let the user choose.
      if (status) status.textContent = '> couldn’t detect your OS automatically — pick a build below';
      if (fallback) fallback.classList.remove('hidden');
      setHero('See downloads', '#download', false);
    }
  }

  /* ---- 3. Reveal-on-scroll ----------------------------------------- */
  function initReveal() {
    var els = document.querySelectorAll('.reveal');
    if (!('IntersectionObserver' in window)) {
      // No support — just show everything.
      els.forEach(function (el) { el.classList.add('in'); });
      return;
    }
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) {
        if (e.isIntersecting) {
          e.target.classList.add('in');
          io.unobserve(e.target);
        }
      });
    }, { threshold: 0.12, rootMargin: '0px 0px -40px 0px' });
    els.forEach(function (el) { io.observe(el); });
  }

  /* ---- 4. Copy-to-clipboard for code blocks ------------------------ */
  function initCopyButtons() {
    document.querySelectorAll('.copy-btn').forEach(function (btn) {
      btn.addEventListener('click', function () {
        var pre = btn.parentElement.querySelector('code');
        var text = pre ? pre.innerText : '';
        var done = function () {
          var original = btn.textContent;
          btn.textContent = 'Copied';
          btn.classList.add('copied');
          setTimeout(function () {
            btn.textContent = original;
            btn.classList.remove('copied');
          }, 1500);
        };
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(done).catch(function () { legacyCopy(text); done(); });
        } else {
          legacyCopy(text);
          done();
        }
      });
    });
  }

  function legacyCopy(text) {
    var ta = document.createElement('textarea');
    ta.value = text;
    ta.setAttribute('readonly', '');
    ta.style.position = 'absolute';
    ta.style.left = '-9999px';
    document.body.appendChild(ta);
    ta.select();
    try { document.execCommand('copy'); } catch (e) { /* ignore */ }
    document.body.removeChild(ta);
  }

  /* ---- 5. Hero screenshot (swap in over the mock once it loads) ---- */
  function initHeroShot() {
    var shot = document.getElementById('heroShot');
    var mock = document.getElementById('heroMock');
    if (!shot) return;
    function showShot() {
      shot.classList.remove('hidden');
      if (mock) mock.classList.add('hidden');
    }
    // Already in cache with real dimensions → swap now; else wait for load.
    if (shot.complete && shot.naturalWidth > 0) {
      showShot();
    } else {
      shot.addEventListener('load', function () {
        if (shot.naturalWidth > 0) showShot();
      });
      // On error (no screenshot file present) we simply keep the mock visible.
    }
  }

  /* ---- 6. Misc ----------------------------------------------------- */
  function initYear() {
    var y = document.getElementById('year');
    if (y) y.textContent = String(new Date().getFullYear());
  }

  /* ---- boot -------------------------------------------------------- */
  function init() {
    applyOS(detectOS());
    initReveal();
    initCopyButtons();
    initHeroShot();
    initYear();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
