# 🦞 Twitter + Hacker News Launch Playbook

> Tactical, dated, attention-first. Complement to `launch-kit.md`. The kit
> has the **copy**; this one has the **execution plan + timing + how to
> hack the algorithms**.
>
> **Last calibrated:** 2026-05-18 using the public X recommendation
> algorithm (re-published Jan + May 2026) and the latest community
> analysis of Show HN front-page outcomes.

---

## ⚡ TL;DR — the two rules that beat everything

1. **X 2026 reality:** **replies are worth ~150× likes** as a ranking signal.
   Optimize for *conversation*, not impressions. Threads + active replies
   in the first hour > polished one-shot tweets.
2. **HN 2026 reality:** there is no magic "best time." Front-page odds are
   set in the **first 60 minutes**: clear non-superlative title +
   substantive first comment + you being in the thread answering
   skeptics. Submit Tue–Thu 09:00–11:00 ET *because you can be present*,
   not because the algorithm rewards it.

Everything else in this doc is a multiplier on those two truths.

---

## Posting cadence (high-level)

| Day | Action |
|---|---|
| **D-3** | Pre-launch warmup posts (see §1.4). Build small backlog of evergreen replies on others' posts to stop being a "ghost account" for X reach. |
| **D-1 evening** | Schedule no posts. Sleep. Have demo video, GitHub release, README, and DMG ready. |
| **D 09:00 ET** | **HN Show HN goes live first.** Pre-write the first comment. |
| **D 09:05 ET** | First comment posted under your own HN submission. |
| **D 09:15 ET** | **X thread goes live** (tweet 1 with video). Send a *casual* mention to ~5 friends to drop early replies. **No "please upvote my HN" — that's a ban.** |
| **D 09:15 → 13:00 ET** | Reply to every HN + X comment within 5 min. This is the entire window that matters. |
| **D 14:00 ET** | If HN trending: do nothing differently. If HN not moving: post into r/macapps + r/SideProject (separate copy each, see launch-kit.md). |
| **D 20:00 ET** | Recap tweet ("update: 200 stars, 12 issues from real users, here's what we learned"). Pull a fresh wave of attention. |
| **D+1 morning** | If HN landed front page → write *retrospective* post (HN loves these). If not → 中文版本 of the X thread for APAC time zones. |
| **D+3** | Ship a small fix from feedback received, tweet about it ("based on @user's feedback in the HN thread, v0.1.27 ships X"). Closes the loop, generates a 2nd wave. |
| **D+7** | Indie Hackers writeup: "What I learned launching MouseClaw on HN" — drives long-tail traffic for months. |

---

# Part 1 · X (Twitter) Playbook

## 1.1 The 2026 X algorithm (what actually matters)

xAI made the source public in Jan 2026 and updated it in May. Relevant
weights, ranked:

| Signal | Approx weight | Action |
|---|---|---|
| **Reply received** | **~150×** baseline | Threads + asking questions. Replies > likes by orders of magnitude. |
| **Thread completion rate** | New 2025 ranking signal | Write 7–10 tweet threads where each is interesting on its own. |
| **Dwell time** | Heavy weight | Native video. Long readable text. No external links in tweet 1. |
| **Reposts** | Moderate | Quotable lines (one per tweet). |
| **Likes** | Low (devalued in 2025) | Don't optimize. |
| **External links in main tweet** | **Penalty** | Always put links in **reply to your own tweet**. |
| **First-hour engagement** | Multiplier on everything | Be present + reply within 5 min. |
| **Niche signal match** | Strong | Use #macOS + #buildinpublic in the bio, not in tweets. |

## 1.2 Account hygiene (D-7 to D-1)

- [ ] **Bio:** under 160 chars, lead with the project, end with a CTA.
  Example: `Building MouseClaw 🦞 — a desktop AI pet for macOS · circle anything, ask anything · free + open source · github.com/edwin-hao-ai/MouseClaw`
- [ ] **Pinned tweet:** the demo MP4. Refreshes whenever a milestone hits.
- [ ] **Banner:** picker screenshot or hero from `docs/assets/hero.png`.
- [ ] **Username:** ideally @MouseClaw or @MouseClawApp. If taken, prefix
  with your own handle's brand.
- [ ] **No external link in the bio if the account is < 10K followers** —
  X has been observed to depress reach for accounts with linktree-style
  bios. Use a single, **clean GitHub URL** instead.
- [ ] **Verify your @ is searchable** — test "mouseclaw" from a logged-out
  browser; you should appear in the autocomplete.

## 1.3 The viral launch thread structure (10 tweets, proven 2026)

Every tweet under 280 chars (or use X Premium for longer, but reach is
nominally lower on long-form unless from verified accounts you'd recognize).

```
Tweet 1 · HOOK + VIDEO
   - Open with a question or contrarian statement
   - Demo video attached (always native upload, never YouTube link)
   - No external links in this tweet (penalty)
   - End with a curiosity gap

Tweet 2 · THE PAIN
   - Concrete, relatable. Show you understand the reader's day.
   - Not "ChatGPT is bad". Say "I had to alt-tab and explain X".

Tweet 3 · THE FIX
   - One-sentence what-it-does. Imperative voice.
   - "Hold X. Drag. Ask. Done."

Tweet 4–7 · CONCRETE SCENARIOS
   - One scenario per tweet. Pictures > words.
   - Each tweet a complete idea (someone landing here from a quote
     tweet should still get the gist).
   - Mix: identify / summarize / execute / type. Different verbs.

Tweet 8 · THE BONUS
   - The "oh wow it also does this?" tweet.
   - Listen carefully — this is where most people hit the like + reply.

Tweet 9 · TECH STACK + TRUST
   - For HN crowd cross-pollinating. "Tauri 2 + Rust + sherpa-onnx.
     Signed + notarized."
   - Mention the OSS + free + privacy angle.

Tweet 10 · CTA (LINK IN REPLY)
   - "⭐ if this is something you'd use:" — leave the link OUT of this tweet
   - As a REPLY to tweet 10, post the GitHub link

   POST AS REPLY TO TWEET 10:
   "Repo + DMG: github.com/edwin-hao-ai/MouseClaw"
```

> Why the link goes in a reply: X penalizes external links in main posts.
> The hack documented by creators in 2026 is to post the **take** in the
> main thread → link in reply. Reach stays intact; click-through *rises*
> because curiosity built up first.

## 1.4 Pre-launch warmup (D-7 to D-1)

This stops the algorithm from treating your launch as a cold-start.

**3 days before launch, post 1–2 small "build in public" tweets:**

```
Example:
"Working on the pet picker UI for MouseClaw. Six pets felt like
enough — turns out a fox makes everything better 🦊"

[screenshot of picker]
```

```
"Just landed: long-press fn anywhere on Mac → speak → words appear
at your cursor. No app switch. Bypasses your IME so 中文 input
methods don't intercept. Hardest part wasn't the audio, it was the
CGEventTap callbacks."
```

Goals:
1. Build a *tiny* base of replies on your most recent tweets so the
   launch isn't your first activity in weeks.
2. Train your followers to expect demo content.
3. Get your username surfaced in the small clusters of people that
   follow you and reply often.

## 1.5 The first 60 minutes (the only window that matters)

Pre-arrange ~5 friends (preferably people active on X) to be alerted at
launch time. NOT to upvote/like — to **reply with a genuine thought or
question**. Replies feed the algorithm 150× more than likes.

What good early replies look like:

```
✅ "what happens if I circle multiple things at once?"
✅ "do you have an Intel build planned? I'm still on a 2019 MBP"
✅ "the typing IME is genius — does it work in Cursor?"

❌ "🔥🔥🔥"
❌ "this is awesome bro"
❌ literal hype with no content
```

The first form generates a **reply chain** (you answer their question,
they reply back, dwell time climbs). The second contributes nothing the
ranker can use.

**Your job in the first hour:**

- [ ] Reply to every comment within 5 minutes
- [ ] Quote-tweet the demo onto your own pinned tweet again (lets the
      algorithm re-rank)
- [ ] Don't ask people to retweet. Don't @ celebrities.
- [ ] Don't tweet anything else. Let the thread breathe.

## 1.6 Anti-patterns (X will punish you)

- ❌ Putting a GitHub link in tweet 1 (≈ 30% reach loss)
- ❌ Posting same content from two accounts (cross-account boost detected,
  shadow-banned)
- ❌ Buying engagement (X audits new viral threads since 2024)
- ❌ Using > 5 hashtags (was 30 in 2019; now diminishing returns past 3)
- ❌ "1/10 🧵" emoji thread markers in 中文 tweets (English audiences are
   used to it; 中文 X 用户更反感)
- ❌ Reply guy energy on celebrities (mute-button trigger; reach to your
   own followers gets quietly reduced)

## 1.7 Day +1 to +7 — extending the half-life

X virality has a ~6-hour half-life. To extend:

| Day | Tactic |
|---|---|
| D+1 | Quote-tweet the original with a *new* angle ("the most surprising scenario from yesterday: ..."). Different hook, same demo. |
| D+2 | Reply to the launch thread with a feedback-driven update ("v0.1.26.1 ships X based on @user's comment"). Re-surfaces it. |
| D+3 | Drop the 中文版本 thread. APAC audience. |
| D+5 | Post a "what I'd do differently" / metrics tweet ("847 stars in 4 days. Here's what surprised me:"). Indie crowd loves these. |
| D+7 | Quote-tweet anyone using your product. Build receipts. |

## 1.8 Drafting the threads — see also

Full pre-written EN + 中文 thread copy lives in
[`launch-kit.md`](launch-kit.md#3-twitter--x). This playbook is the
*execution wrapper* around it.

---

# Part 2 · Hacker News Playbook

## 2.1 What HN actually is in 2026

- A **single-page algorithmically-ranked feed** of submissions.
- ~5M monthly uniques. Spike traffic of **10K–50K visits** for a
  front-page Show HN.
- Audience: **deeply technical**, **skeptical**, **anti-marketing**.
  They will read your README. They will run your code. They will find
  the bug in your authentication flow.
- Truth ranks higher than polish. **A candid "here's what doesn't work
  yet" earns more karma than a perfect launch.**

## 2.2 What "front page" actually means

```
new submissions land in   →   /newest        (visible for ~30 min)
those with early upvotes  →   /new           (visible for ~2 hours)
those that survive /new   →   / (front page) (visible for ~10–24 hours)
```

**The bottleneck is /newest → /new.** A handful of early upvotes from
real HN accounts in your first 30 minutes is the single biggest predictor
of whether your post climbs. **You do not buy these.** They come from
posting genuinely interesting work + having a non-clickbait title +
being there to engage.

## 2.3 Timing — the honest answer

Multiple 2025–2026 analyses contradict each other. What's actually
consistent:

| Window | Why |
|---|---|
| **Tue / Wed / Thu, 09:00–11:00 ET** | Most US tech audience awake + you can be in the thread for the next 4 hours. **Default to this.** |
| **Sun 00:00–01:00 PT** | Lowest competition (per June 2025 23K-post analysis). Risky but contrarian-viable if you're confident. |
| **Anytime else** | Fine if you have a genuinely strong submission. Time matters less than the title + first comment. |

**Avoid:** weekday evenings ET (HN front page is already locked by
established threads), Fridays after 14:00 ET (audience checks out).

## 2.4 Title craft (under 80 chars, no caps, no hype)

✅ **Good:**
```
Show HN: MouseClaw – a pixel-art desktop AI pet for macOS (hold to speak)
```

❌ **Bad — would get auto-flagged:**
```
🔥 MouseClaw 🦞 the BEST desktop AI ever - HUGE update! 🚀
```

❌ **Bad — too vague:**
```
Show HN: I built a desktop AI thing
```

**Rules HN dang has stated:**

- No superlatives (best, fastest, biggest, first, only, ultimate)
- No emojis or all-caps
- No "We are launching" / "Excited to announce"
- Past tense or present participle works
- Include `Show HN:` prefix if you can demo / run it
- Include `Ask HN:` if it's a question
- 50–70 chars is the sweet spot

## 2.5 The first comment is your real pitch

The title gets you in the door. The first comment is what gets people
upvoting. Post it yourself, immediately after submitting.

**Template that works** (steal liberally):

```
Hi HN — I built MouseClaw because [PERSONAL PAIN].

How it works:
- [bullet about main feature]
- [bullet about main feature]
- [bullet about main feature]

Stack: [tech specifics — HN loves this]

Two things I'd love feedback on:
1. [specific design decision]
2. [specific tradeoff]

[Repo link]
[Demo link]
```

> The full pre-written MouseClaw Show HN comment lives in
> [`launch-kit.md` §1](launch-kit.md#1-hacker-news--show-hn-english-only).

### Why this format works

- **"Because I had X pain"** = you're a person, not a startup
- **Stack:** = HN's love language
- **Specific feedback asks** = invites comments, not silent upvotes
- **Repo + demo link** = if commenter is convinced by minute 2, they
  click without scrolling
- **No CTA, no "please upvote"** = the moment you do this you're flagged

## 2.6 The first 4 hours

Same window of life-or-death as X, but the verbs differ:

| Time | Action |
|---|---|
| **Submit** | Post submission. |
| **+ 2 min** | Post first comment yourself. |
| **+ 5 min** | Refresh /newest in incognito. If your post is there, leave it alone. **Do NOT click your own link / refresh excessively.** HN flags suspicious traffic. |
| **+ 10–60 min** | Watch for comments. Reply to **every single one** within 10 min, especially the negative/critical ones. **HN punishes silent founders.** |
| **+ 30 min** | Check /new. If you're there, you're winning. |
| **+ 1 hr** | If you've made /, post a single tweet linking to the HN thread ("on the front page of HN today, would love your thoughts" — drives more comment activity). |
| **+ 4 hr** | Posts typically peak around this point. You should still be replying. |

## 2.7 Replying to skeptics (this is where karma is made)

HN comments will include:
- "Why didn't you use X instead?"
- "How is this different from Y?"
- "Doesn't this have problem Z?"

**The structure that wins:**

```
Good question — [acknowledge the valid part of their concern].

[Honest, specific answer including the trade-off you made.]

[If you don't know yet:] "I don't have great data on this yet — if
you do try it, that's exactly the feedback I'm looking for."
```

**The structure that loses:**

```
Actually, [defensive correction]. [Marketing speak].
```

The HN community **will see this** and turn against you. Real example:
in 2024 a startup hit #1 for 6 hours, then a dev got defensive in
replies, and within 2 hours the thread was buried with flags.

## 2.8 Anti-patterns (will get you penalized)

- ❌ **"Please upvote!" anywhere** — automatic ban (vote rings detection)
- ❌ **Multiple accounts** — IP-fingerprinted, instant nuke
- ❌ **Editorializing your own submission** ("amazing tool I built")
- ❌ **Linking to a marketing landing page instead of the repo/demo**
- ❌ **Going silent for > 30 min during peak hours**
- ❌ **Deleting comments / hiding criticism** — visible in the thread
  metadata, screenshots get posted to /r/programming
- ❌ **Re-submitting the same URL** within 7 days (it'll be auto-killed)

## 2.9 If you flop (didn't make front page)

This happens ~70% of the time even with perfect execution. **It's fine.**

- [ ] Wait **at least 1 week** before re-submitting (HN dang's stated policy)
- [ ] Change the URL slightly (e.g., `?ref=hn2`) so it doesn't auto-merge
- [ ] Improve the title based on what you learned
- [ ] Move on. Don't spam HN with re-launches.

Better alt path: when you ship a meaningful new version, do a
**second Show HN as "Show HN: MouseClaw v0.2 – now does X"** with a
genuinely different angle. dang is OK with this if it's a real new
artifact.

## 2.10 If you make front page

```
Hour 0:    submit + first comment (you, alone)
Hour 1:    front page · ~500 visits/hr
Hour 4:    peak · ~3-8K visits/hr · expect 30-100 comments
Hour 8:    starting to slide off · still ~1-2K/hr
Hour 24:   ~10K-50K total visits · DMG downloads spike
Hour 48:   archived to past front pages · still drives long-tail
Hour 168:  HN Daily / Best Of writeups may surface you again
```

**Things to do while you're trending:**

- [ ] Reply, reply, reply. For the first 8 hours minimum.
- [ ] Don't drop the X link in HN comments (frowned on; do reverse only)
- [ ] Tweet the HN link about an hour in: "We're on HN! Would love your
      thoughts in the comments: news.ycombinator.com/item?id=..."
- [ ] Have your GitHub README perfect: hero image, install command,
      demo GIF above the fold
- [ ] **Server: be ready for the spike.** If you have any cloud component,
      double its capacity now. MouseClaw is a local-only app — relevant
      only for the GitHub Pages landing page (Cloudflare handles it).
- [ ] **Check your DMG download counts via `gh release view`** — this is
      the real signal of conversion vs. just-curious traffic.

---

# Part 3 · Cross-platform amplification

## 3.1 HN → X (preferred direction)

If HN takes off:
1. Wait ~60 min after submitting (let HN ranking settle)
2. Tweet: "Show HN'd MouseClaw an hour ago and you all are too kind 🦞
   [link to HN thread]"
3. Tweet engagement explodes because *HN traffic carries social proof*
4. HN viewers who don't comment on HN often comment on X — you double
   your reply count

## 3.2 X → HN (riskier — be careful)

If X is going viral:
1. **Do NOT submit to HN** with "trending on Twitter!!" anywhere
2. Wait until next Tuesday morning. Post a fresh Show HN with a tech
   angle (not the viral angle). HN auto-detects when something's been
   "discovered elsewhere already" and ranks it lower

## 3.3 The 24-hour news cycle

Both X and HN have ~24-hour cycles. By coordinating launch *day*, you
get one big spike. By *separating them by a week*, you get two spikes
of similar size — better for indie launches because you can't sustain
attention on either platform for that long anyway.

**Recommended cadence:**
- Week 1: HN Show HN + lightweight X teaser
- Week 2: full X thread (treating HN-driven feedback as new content)
- Week 3: 中文 X thread + 小红书
- Week 4: Indie Hackers writeup ("what I learned launching")

---

# Part 4 · KPIs to track (and what they mean)

| Metric | Source | Healthy launch | What it means if low |
|---|---|---|---|
| **HN points (24h)** | the post itself | 200+ = front page, 500+ = #1-3 | Title or first comment wasn't compelling |
| **HN comments** | the post | 30+ | People didn't connect with the framing |
| **HN → repo clicks** | `?ref=hn` UTM | 5-10% of post visits | Demo / hero didn't sell |
| **GitHub stars (D+7)** | `gh repo view` | 100+ for a niche tool, 1000+ for general | Tool isn't sticky |
| **DMG downloads (D+7)** | `gh release view` | 30-50% of stars | Install friction (Gatekeeper, "weird app", etc.) |
| **X thread impressions** | analytics | 50K+ | Hook didn't land |
| **X thread reply count** | manual | 30+ | Thread wasn't conversational |
| **X profile follows** | analytics | 10% of impressions = good | Profile bio doesn't convert |

**The signal we actually care about (long-term):** **DMG re-opens** —
i.e., did the user install AND come back? GitHub doesn't measure this.
You only see it via auto-update check pings to `version.json` on GitHub
Pages. Set up a Cloudflare-side analytics on that endpoint and watch
the rolling 7-day retention. **That's your real product-market-fit
signal**, not launch buzz.

---

# Part 5 · Anti-patterns checklist (printable)

Before hitting submit on either platform, run this:

**Before HN submit:**

- [ ] Title under 80 chars, no caps, no emojis, no superlatives
- [ ] Title has `Show HN:` prefix
- [ ] First comment is drafted and copied to clipboard
- [ ] Repo README has demo GIF above the fold
- [ ] Repo has a clean install instruction at the top
- [ ] No "please upvote" / "we just launched" / "excited to announce" anywhere
- [ ] I will be at my desk for the next 4 hours

**Before X thread:**

- [ ] Tweet 1 has demo video attached (native upload, not YouTube)
- [ ] Tweet 1 has NO external link
- [ ] All tweets under 280 chars
- [ ] Last tweet's link is in a REPLY to that tweet, not in it
- [ ] My pinned tweet is also fresh
- [ ] I have 5 friends warmed up to reply (not like) in the first hour
- [ ] I have the full thread pre-drafted (don't compose live)
- [ ] My bio matches the launch (no stale tagline)

---

# Part 6 · The single most-asked question

> "Should I do HN or X first?"

**HN, by ~30 minutes.** Here's why:

- HN traffic is **high-conviction + high-context**. People who upvote
  on HN often install + star the repo same hour. This populates your
  GitHub social proof.
- An hour later, when X kicks in, your repo already has 100 stars and
  10 issues — much more credible than the "0 stars 0 issues" repo.
- HN to X amplification is positive (HN → X feels like "I deserved
  this attention"). X to HN amplification is risky (HN detects
  Twitter-driven traffic and de-ranks).

If you want to test only one: **HN.** A successful Show HN turns into
a Tweet thread by itself (other people will quote it).

---

## References (2026, verified)

- [How X (Twitter) Algorithm Works in 2026 (Source Code)](https://posteverywhere.ai/blog/how-the-x-twitter-algorithm-works) — the 150× replies signal, link penalty, thread completion rate
- [X Open-Sourced Its Algorithm on GitHub: What the Code Actually Says](https://opentweet.io/blog/x-algorithm-open-source-github-2026)
- [The best time to post on Hacker News (2025 analysis)](https://blog.alcazarsec.com/tech/posts/best-time-to-post-on-hacker-news)
- [How to crush your Hacker News launch (dev.to/dfarrell)](https://dev.to/dfarrell/how-to-crush-your-hacker-news-launch-10jk)
- [How to launch a dev tool on Hacker News (markepear.dev)](https://www.markepear.dev/blog/dev-tool-hacker-news-launch)
- [Hacker News Posting Guide: Rules, Show HN, and Timing](https://syften.com/blog/hacker-news-marketing/)
- [Front page of HN: the full postmortem (Indie Hackers)](https://www.indiehackers.com/post/front-page-of-hn-the-full-postmortem-traffic-lessons-surprises-cbe9e0a7f6)
- [How to launch a startup on Hacker News (Marc Lou's Just Ship It)](https://newsletter.marclou.com/p/how-to-launch-a-startup-on-hacker-news)
- [Show HN front page strategy (Indie Hackers)](https://www.indiehackers.com/post/my-show-hn-reached-hacker-news-front-page-here-is-how-you-can-do-it-44c73fbdc6)
- [Twitter Threads viral examples 2026](https://aifreeforever.com/blog/15-best-twitter-thread-examples-that-went-viral)

---

## Companion docs

- [`docs/marketing/launch-kit.md`](launch-kit.md) — the actual EN + 中文 copy for HN, X, Reddit, YouTube, Instagram, 小红书
- [`docs/design/v0.1.26.md`](../design/v0.1.26.md) — what shipped
- [`CHANGELOG.md`](../../CHANGELOG.md) — what to reference in the launch

---

> 🦞 **The meta-principle.** Both platforms reward **being a person, not
> a brand**. You can't fake it in 2026 — the algorithms have learned
> what "marketing speak" looks like and quietly throttle it. Write the
> way you'd describe MouseClaw to a friend over coffee. That's it. Every
> tactic above is just scaffolding around that one truth.
