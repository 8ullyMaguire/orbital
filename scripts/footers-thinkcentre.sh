#!/usr/bin/env bash
# Put the Orbital footer band on every SFW site served by this host's nginx.
#
#   ./scripts/footers-thinkcentre.sh            # apply
#   ./scripts/footers-thinkcentre.sh --revert   # remove
#
# Why nginx and not each app's templates: ten sites, ten codebases, ten build
# pipelines. Injecting one band at the edge is one change, reversible in one
# command, and it cannot drift when a site is rebuilt. The band is inline-styled
# so it needs nothing from the page it lands in.
set -euo pipefail

HOST="${HOST:-thinkcentre}"
SITES="${SITES:-ascend blackrep blog gravity gravity-8015 gravity-prod2 kindling lorehaven piefed stash-box suki whitebois radio}"
MARK="orbital-footer"
SNIPPET="/etc/nginx/snippets/orbital-footer.conf"
REVERT=0
[ "${1:-}" = "--revert" ] && REVERT=1

if [ "$REVERT" = 1 ]; then
  ssh "$HOST" "sudo bash -s" <<REMOTE
set -euo pipefail
for s in $SITES; do
  f=/etc/nginx/sites-available/\$s
  [ -f "\$f" ] || continue
  if grep -q '$MARK' "\$f"; then
    sudo sed -i '/$MARK/d' "\$f"
    echo "reverted \$s"
  fi
done
sudo nginx -t && sudo systemctl reload nginx
REMOTE
  exit 0
fi

ssh "$HOST" "sudo bash -s" <<REMOTE
set -euo pipefail
mkdir -p /etc/nginx/snippets
cat > $SNIPPET <<'SNIP'
# Injected into every SFW site's HTML by footers-thinkcentre.sh. Inline styles,
# no external request, no layout assumptions: it appends one line at the end of
# the body and nothing else.
proxy_set_header Accept-Encoding "";
sub_filter_once on;
sub_filter_types text/html;
sub_filter '</body>' '<div style="padding:14px 20px;text-align:center;font:13px/1.5 ui-sans-serif,system-ui,-apple-system,Segoe UI,Roboto,sans-serif;color:#8d93b0;background:#06070d;border-top:1px solid #ffffff14"><a href="https://links.polarisocial.xyz/?s=footer" style="color:#b3a6ff;text-decoration:none">Built in public &middot; the people behind these projects are listed on Orbital &#8594;</a></div></body>';
SNIP

for s in $SITES; do
  f=/etc/nginx/sites-available/\$s
  [ -f "\$f" ] || { echo "skip \$s (no site file)"; continue; }
  if grep -q '$MARK' "\$f"; then echo "already \$s"; continue; fi
  # Insert inside the first server block, right after its opening brace.
  awk 'BEGIN{done=0} {print} /^[[:space:]]*server[[:space:]]*\{/ && !done {print "    include $SNIPPET; # $MARK"; done=1}' "\$f" > /tmp/site.new
  sudo mv /tmp/site.new "\$f"
  echo "patched \$s"
done
sudo nginx -t && sudo systemctl reload nginx
REMOTE

echo
echo "verify: curl -s https://<site>/ | grep -c orbital-footer  (expect 1)"
