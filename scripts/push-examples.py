#!/usr/bin/env python3
"""
Push docs/examples/*.html to Valisthea/covenant-lang/examples/ via GitHub API.
Also updates vercel.json and sitemap.xml.
"""

import os, json, base64, urllib.request, urllib.error, time, subprocess, sys

REPO  = "Valisthea/covenant-lang"
BRANCH= "main"
API   = f"https://api.github.com/repos/{REPO}/contents"
TOKEN = subprocess.check_output(["gh", "auth", "token"]).decode().strip()

EXAMPLES_DIR = os.path.join(os.path.dirname(__file__), "..", "docs", "examples")

HEADERS = {
    "Authorization": f"token {TOKEN}",
    "User-Agent":    "covenant-examples-deploy",
    "Accept":        "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
}


def api_get(path):
    url = f"{API}/{path}"
    req = urllib.request.Request(url, headers=HEADERS)
    try:
        resp = urllib.request.urlopen(req)
        return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return None
        raise


def api_put(path, message, content_b64, sha=None):
    url = f"{API}/{path}"
    body = {"message": message, "content": content_b64, "branch": BRANCH}
    if sha:
        body["sha"] = sha
    data = json.dumps(body).encode()
    req  = urllib.request.Request(url, data=data, headers={**HEADERS, "Content-Type": "application/json"}, method="PUT")
    try:
        resp = urllib.request.urlopen(req)
        result = json.loads(resp.read())
        return result
    except urllib.error.HTTPError as e:
        err = e.read().decode()
        print(f"  ERROR {e.code}: {err[:200]}")
        raise


def push_file(local_path, repo_path, commit_msg):
    with open(local_path, "rb") as f:
        raw = f.read()
    b64 = base64.b64encode(raw).decode()

    existing = api_get(repo_path)
    sha = existing["sha"] if existing else None
    action = "Update" if sha else "Add"

    print(f"  {action}: {repo_path} ({len(raw):,} bytes) ...", end=" ", flush=True)
    api_put(repo_path, commit_msg, b64, sha)
    print("OK")
    time.sleep(0.4)   # gentle rate limiting


def main():
    files = sorted(f for f in os.listdir(EXAMPLES_DIR) if f.endswith(".html"))
    print(f"\nFound {len(files)} HTML files to push to {REPO}/examples/\n")

    commit_msg = "docs(examples): Covenant By Example, 15 chapters V0.7"

    for filename in files:
        local_path = os.path.join(EXAMPLES_DIR, filename)
        repo_path  = f"examples/{filename}"
        push_file(local_path, repo_path, commit_msg)

    # ── Update sitemap.xml ─────────────────────────────────────────────
    print("\nUpdating sitemap.xml ...")
    existing_sitemap = api_get("sitemap.xml")
    sitemap_sha      = existing_sitemap["sha"] if existing_sitemap else None
    if existing_sitemap:
        current_sitemap = base64.b64decode(existing_sitemap["content"]).decode()
    else:
        current_sitemap = '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n</urlset>'

    # Build new example URLs
    chapter_slugs = [
        ("01","hello-contract"), ("02","fields-and-storage"), ("03","actions-events-errors"),
        ("04","guards"), ("05","external-calls"), ("06","erc20-token"),
        ("07","fhe-basics"), ("08","encrypted-token"), ("09","post-quantum-signatures"),
        ("10","zero-knowledge-proofs"), ("11","cryptographic-amnesia"),
        ("12","uups-upgradeable"), ("13","beacon-proxy"), ("14","oracle-integration"),
        ("15","deploy-to-sepolia"),
    ]
    new_urls = []
    new_urls.append("  <url><loc>https://examples.covenant-lang.org/</loc><priority>0.9</priority></url>")
    for num, slug in chapter_slugs:
        new_urls.append(
            f"  <url><loc>https://examples.covenant-lang.org/{num}-{slug}</loc>"
            f"<priority>0.7</priority></url>"
        )

    # Inject before closing </urlset>
    sitemap_new_entries = "\n".join(new_urls)
    if "examples.covenant-lang.org" in current_sitemap:
        print("  sitemap already contains examples entries, skipping update")
    else:
        new_sitemap = current_sitemap.replace(
            "</urlset>",
            f"\n  <!-- Covenant By Example -->\n{sitemap_new_entries}\n</urlset>"
        )
        b64 = base64.b64encode(new_sitemap.encode()).decode()
        api_put("sitemap.xml", "docs(sitemap): add examples.covenant-lang.org pages", b64, sitemap_sha)
        print("  sitemap.xml updated OK")

    print(f"\nAll done -- {len(files)} example pages pushed.")
    print("  Live at: https://covenant-lang.org/examples/index.html")
    print("  (or https://examples.covenant-lang.org/ once subdomain is configured)")


if __name__ == "__main__":
    main()
