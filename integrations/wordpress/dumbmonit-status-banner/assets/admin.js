/* DumbMonit status banner: "Find status pages" on the settings page. */
(function () {
  var cfg = window.dmtAdmin;
  var button = document.getElementById('dmt-find');
  var base = document.getElementById('dmt-base');
  var slug = document.getElementById('dmt-slug');
  var select = document.getElementById('dmt-pages');
  var note = document.getElementById('dmt-find-note');
  if (!cfg || !button || !base || !slug || !select || !note) return;

  function say(text) {
    note.textContent = text;
  }

  button.addEventListener('click', function () {
    var body = new URLSearchParams();
    body.append('action', 'dmt_discover');
    body.append('nonce', cfg.nonce);
    body.append('base', base.value);
    button.disabled = true;
    say(cfg.i18n.searching);
    fetch(cfg.ajaxUrl, { method: 'POST', credentials: 'same-origin', body: body })
      .then(function (response) {
        return response.json();
      })
      .then(function (reply) {
        if (!reply || !reply.success) {
          say(reply && reply.data && reply.data.message ? reply.data.message : cfg.i18n.failed);
          return;
        }
        var pages = (reply.data && reply.data.pages) || [];
        while (select.options.length) select.remove(0);
        select.add(new Option(cfg.i18n.choose, ''));
        pages.forEach(function (page) {
          var label = page.title === page.slug ? page.slug : page.title + ' (' + page.slug + ')';
          var current = page.slug === slug.value;
          select.add(new Option(label, page.slug, current, current));
        });
        select.hidden = pages.length === 0;
        if (reply.data.base) base.value = reply.data.base;
        say(pages.length ? cfg.i18n.pick : cfg.i18n.none);
        if (pages.length) select.focus();
      })
      .catch(function () {
        say(cfg.i18n.failed);
      })
      .then(function () {
        button.disabled = false;
      });
  });

  select.addEventListener('change', function () {
    if (select.value) slug.value = select.value;
  });
})();
