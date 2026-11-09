import json
import os
FS = "Fighter Screen.json"; ABC = "Aslan Border Clash.json"; SW = "Fighter Swarm.json"; KH = "Keelhaven.json"
PK = "Picket at the Refuelling Point.json"; CV = "Convoy Escape.json"; TB = "Thebus.json"
LF = "Light Fighter"; KIN = "Colonial Cruiser - Kinunir"; PW = "Pocket Warship - Halaheike"
DE = "Destroyer Escort - Chrysanthemum"; EE = "Ekawsiykua Escort"; MC = "Mercenary Cruiser - Type C (armed)"
PC = "Patrol Corvette - Type T"
FS_SQ = {"Lance": ["Lance"], "Flag": ["CS Hesperus"]}
FSR_SQ = {"Spear": ["Spear"], "Flag": ["CS Tarsis"]}
ABC_SQ = {"Escorts": ["IMS Steadfast", "IMS Valiant"], "Corvettes": ["IMS Harrier", "IMS Lark", "IMS Merlin", "IMS Osprey"],
          "Boats": ["IMS Basilisk", "IMS Drake", "IMS Lindworm", "IMS Wyvern"]}
ABCR_SQ = {"Warships": ["Aokhoiyeal", "Ftahkoa"], "Escorts": ["Eakhtau", "Hkasoi", "Khtahea", "Yerlyaoi"], "Iyre": ["Iyre"]}
SW_SQ = {"Vigilant": ["IMS Vigilant"], "Hand": ["Iron Hand"]}
SWR_SQ = {"Alpha": ["Talon 0"], "Bravo": ["Talon 1", "Talon 2"]}
PK_SQ = {"Picket": ["IMS Vigilant"], "Boats": ["SDB"]}
CV_SQ = {"Escort": ["IMS"], "Convoy": ["Void Trader"]}
KH_SQ = {"Battle Line": ["Sirocco", "Intrepid", "Dauntless"], "Screen": ["Arcturus", "Ishkur", "Ninurta", "Taipan", "Mamba", "Krait"]}
CANT = ["cannot", "can't", "can not", "unable", "not possible", "isn't possible", "no way", "not able",
        "not supported", "no order", "no means", "nearest"]


def c(who, path, step="first", what=None, **test):
    d = {"who": who, "path": path, "step": step}
    for k, v in test.items():
        d[{"in_": "in", "not_": "not"}.get(k, k)] = v
    if what:
        d["what"] = what
    return d


def any_of(*cs, what=None):
    d = {"any_of": list(cs)}
    if what:
        d["what"] = what
    return d


def all_of(*cs, what=None):
    d = {"all_of": list(cs)}
    if what:
        d["what"] = what
    return d


def eng_design(who, design, step="first", alt=None, what=None):
    opts = [c(who, "engage.designs", step, contains=design)] + (alt or [])
    return any_of(*opts, what=what or f"{who} engages {design}")


def eng_fighters(who, step="first"):
    return eng_design(who, LF, step, [c(who, "engage.max_tons", step, lte=100)], what=f"{who} engages fighters")


def eng_free(who, step="all"):
    return c(who, "engage", step, absent=True, what=f"{who} has no engage limit")


def goal(who, g, step="all"):
    return c(who, "goal", step, eq=g, what=f"{who} goal {g} ({step})")


def steps_gte(who, n):
    return {"who": who, "steps_gte": n, "what": f"{who} has {n}+ steps"}


cases = []


def case(id, cat, sc, side, sq, orders, checks):
    cases.append({"id": id, "category": cat, "scenario": sc, "side": side, "squadrons": sq,
                  "orders": orders, "checks": checks})


# --- plain goals
case("fs-destroy-all", "goal", FS, "Blue", FS_SQ, {"fleet": "Attack. Destroy everything they have."},
     [goal("all", "destroy"), eng_free("all")])
case("abc-red-raid", "goal", ABC, "Red", ABCR_SQ, {"fleet": "This is a raid: hurt them badly, then get out."},
     [any_of(goal("all", "raid", "first"), goal("all", "escape", "last"), what="raid, or fight then escape")])
case("sw-red-fight", "goal", SW, "Red", SWR_SQ, {"fleet": "Fight it out. No retreat."},
     [goal("all", "destroy"), eng_free("all")])

# --- hold with a post
case("pk-hold-kherra", "hold", PK, "Blue", PK_SQ, {"fleet": "Hold position at Kherra."},
     [goal("all", "hold"), c("all", "post.planet", "all", eq="Kherra", what="post Kherra")])
case("pk-hold-here", "hold", PK, "Blue", PK_SQ, {"fleet": "Stay where you are and let them come to you."},
     [goal("all", "hold"), c("all", "movement.mode", "all", eq="network", what="no set movement"),
      c("all", "post.planet", "all", absent=True, what="no planet post")])
case("fs-lance-guard-flag", "hold", FS, "Blue", FS_SQ, {"squadron": {"Lance": "Stay with the cruiser and guard her."}},
     [goal("Lance", "hold"),
      any_of(c("Lance", "post.ship", eq="CS Hesperus"), c("Lance", "post.squadron", eq="Flag"), what="Lance posted on the cruiser"),
      goal("Flag", "destroy"), eng_free("Flag")])
case("cv-hold-radius", "hold", CV, "Blue", CV_SQ, {"fleet": "Hold within 5,000 km of Kaldor."},
     [goal("all", "hold"), c("all", "post.planet", "all", eq="Kaldor", what="post Kaldor"),
      c("all", "post_radius_km", "all", eq=5000, what="radius 5,000 km")])
case("fsr-spear-screen", "hold", FS, "Red", FSR_SQ, {"squadron": {"Spear": "Screen the cruiser. Don't go chasing."}},
     [goal("Spear", "hold"),
      any_of(c("Spear", "post.ship", eq="CS Tarsis"), c("Spear", "post.squadron", eq="Flag"), what="Spear posted on Tarsis"),
      c("Spear", "movement.mode", "all", eq="network", what="Spear no set movement"), goal("Flag", "destroy")])

# --- targets
case("fs-lance-fighters", "target", FS, "Blue", FS_SQ, {"squadron": {"Lance": "Kill their fighters."}},
     [eng_fighters("Lance"), eng_free("Flag"), goal("Lance", "destroy")])
case("fs-flag-cruiser", "target", FS, "Blue", FS_SQ, {"squadron": {"Flag": "Engage only the enemy cruiser."}},
     [any_of(c("Flag", "engage.ships", contains="CS Tarsis"), c("Flag", "engage.designs", contains=KIN),
             c("Flag", "engage.min_tons", gte=11), what="Flag engages Tarsis"),
      eng_free("Lance")])
case("fs-lance-strict", "target", FS, "Blue", FS_SQ,
     {"squadron": {"Lance": "Fire only at their fighters -- nothing else, even if the cruiser shoots at you."}},
     [eng_fighters("Lance"), c("Lance", "threats", "all", eq=0, what="Lance threats 0"), eng_free("Flag")])
case("abc-big-ships", "target", ABC, "Blue", ABC_SQ, {"fleet": "Everyone concentrate on their big ships, the ones over 1,000 tons."},
     [any_of(all_of(c("all", "engage.min_tons", gte=801), c("all", "engage.min_tons", lte=1200)),
             c("all", "engage.designs", contains=PW),
             all_of(c("all", "engage.ships", contains="Aokhoiyeal"), c("all", "engage.ships", contains="Ftahkoa")),
             what="all engage the pocket warships"),
      c("all", "engage.designs", "all", contains=EE, not_=True, what="800 t escorts not included")])
case("abc-escorts-ftahkoa", "target", ABC, "Blue", ABC_SQ, {"squadron": {"Escorts": "Take out Ftahkoa first."}},
     [c("Escorts", "engage.ships", contains="Ftahkoa", what="Escorts first engage Ftahkoa"), eng_free("Corvettes"), eng_free("Boats")])
case("abc-small-craft", "target", ABC, "Blue", ABC_SQ, {"squadron": {"Corvettes": "Deal with the small craft."}},
     [eng_fighters("Corvettes"), eng_free("Escorts"), eng_free("Boats")])
case("abc-three-way", "target", ABC, "Blue", ABC_SQ,
     {"fleet": "Corvettes on their fighters, escorts on their escorts, boats on the warships."},
     [eng_fighters("Corvettes"), eng_design("Escorts", EE), eng_design("Boats", PW, alt=[c("Boats", "engage.min_tons", gte=801)])])
case("abc-red-des", "target", ABC, "Red", ABCR_SQ, {"squadron": {"Warships": "Go for the destroyer escorts."}},
     [eng_design("Warships", DE, alt=[all_of(c("Warships", "engage.ships", contains="IMS Steadfast"),
                                             c("Warships", "engage.ships", contains="IMS Valiant"))]),
      eng_free("Escorts"), eng_free("Iyre")])
case("swr-escort", "target", SW, "Red", {}, {"fleet": "Destroy the destroyer escort; ignore the cruiser unless it shoots at you."},
     [eng_design("all", DE, alt=[c("all", "engage.ships", contains="IMS Vigilant")]),
      any_of(c("all", "threats", "all", absent=True), c("all", "threats", "all", gte=1), what="threats allowed (the cruiser, if it shoots)")])

# --- timed arrivals
case("fs-arrive-medium-6", "arrival", FS, "Blue", FS_SQ, {"fleet": "All squadrons close to Medium range of the enemy together by round 6."},
     [c("all", "movement.mode", eq="arrive", what="arrive"), c("all", "movement.band", eq="Medium", what="band Medium"),
      c("all", "movement.round", eq=6, what="round 6")])
case("fs-flag-long-tarsis", "arrival", FS, "Blue", FS_SQ, {"squadron": {"Flag": "Be at Long range of CS Tarsis by round 4."}},
     [c("Flag", "movement.mode", eq="arrive"), c("Flag", "movement.at.ship", eq="CS Tarsis"), c("Flag", "movement.band", eq="Long"),
      c("Flag", "movement.round", eq=4), c("Lance", "movement.mode", "all", eq="network", what="Lance unchanged")])
case("abc-arrive-together", "arrival", ABC, "Blue", ABC_SQ,
     {"fleet": "Escorts and corvettes: arrive at Long range of their warships together on round 5. Boats: stay back."},
     [c("Escorts", "movement.mode", eq="arrive"), c("Escorts", "movement.round", eq=5), c("Escorts", "movement.band", eq="Long"),
      c("Corvettes", "movement.mode", eq="arrive"), c("Corvettes", "movement.round", eq=5), c("Corvettes", "movement.band", eq="Long"),
      any_of(c("Escorts", "movement.at.enemies.designs", contains=PW), c("Escorts", "movement.at.ship", in_=["Aokhoiyeal", "Ftahkoa"]),
             c("Escorts", "movement.at.enemies.min_tons", gte=11), what="at the warships (not fighters)"),
      c("Boats", "movement.mode", "first", in_=["network", "burn"], what="Boats do not arrive")])
case("cv-kaldor-10", "arrival", CV, "Blue", CV_SQ, {"fleet": "Make for Kaldor and be there by round 10."},
     [c("all", "movement.mode", eq="arrive"), c("all", "movement.at.planet", eq="Kaldor"), c("all", "movement.round", eq=10),
      c("all", "goal", "all", in_=["destroy", "hold"], what="no escape")])
case("fs-short-then-fight", "arrival", FS, "Blue", FS_SQ,
     {"squadron": {"Lance": "Get to Short range of their fighters by round 3, then dogfight."}},
     [c("Lance", "movement.mode", eq="arrive"), c("Lance", "movement.band", eq="Short"), c("Lance", "movement.round", eq=3),
      any_of(c("Lance", "movement.at.enemies.designs", contains=LF), c("Lance", "movement.at.enemies.max_tons", lte=100),
             what="at their fighters"),
      steps_gte("Lance", 2), c("Lance", "movement.mode", "last", eq="network", what="then free"),
      c("Lance", "until", absent=False, what="first step ends")])

# --- burns
case("fs-burn-3g", "burn", FS, "Blue", FS_SQ, {"squadron": {"Lance": "Burn toward the enemy fighters at 3G."}},
     [c("Lance", "movement.mode", eq="burn"), c("Lance", "movement.thrust", eq=3),
      any_of(c("Lance", "movement.toward.enemies.designs", contains=LF), c("Lance", "movement.toward.enemies.max_tons", lte=100),
             what="toward their fighters"),
      c("Lance", "movement.away", absent=True, what="not away")])
case("abc-boats-open", "burn", ABC, "Blue", ABC_SQ, {"squadron": {"Boats": "Open the range: burn away from the enemy at full thrust."}},
     [c("Boats", "movement.mode", eq="burn"), c("Boats", "movement.away", eq=True),
      any_of(c("Boats", "movement.thrust", absent=True), c("Boats", "movement.thrust", gte=6), what="full thrust"),
      c("Boats", "dodge", "all", absent=True, what="no dodge (it would stop the run)"),
      c("Escorts", "movement.mode", "all", eq="network", what="Escorts unchanged")])
case("cv-convoy-burn", "burn", CV, "Blue", CV_SQ, {"squadron": {"Convoy": "Burn for Kaldor at 2G."}},
     [c("Convoy", "movement.mode", eq="burn"), c("Convoy", "movement.toward.planet", eq="Kaldor"), c("Convoy", "movement.thrust", eq=2),
      c("Escort", "movement.mode", "all", eq="network", what="Escort unchanged")])
case("pk-burn-until-long", "burn", PK, "Blue", PK_SQ,
     {"fleet": "Burn toward the corsairs at 2G until you are at Long range, then fight as you see fit."},
     [c("all", "movement.mode", eq="burn"), c("all", "movement.thrust", eq=2), c("all", "until.within.band", eq="Long"),
      steps_gte("all", 2), c("all", "movement.mode", "last", eq="network", what="then free")])

# --- dodge
case("abc-corvettes-dodge2", "dodge", ABC, "Blue", ABC_SQ, {"squadron": {"Corvettes": "Keep 2G back for evasion."}},
     [c("Corvettes", "dodge", "all", eq=2), goal("Corvettes", "destroy"), eng_free("Corvettes"),
      c("Escorts", "dodge", "all", absent=True, what="Escorts no dodge")])
case("fs-lance-no-dodge", "dodge", FS, "Blue", FS_SQ,
     {"squadron": {"Lance": "Don't waste thrust dodging; put all your drive into manoeuvre."}},
     [c("Lance", "dodge", "all", eq=0), eng_free("Lance")])
case("sw-dodge-all", "dodge", SW, "Blue", SW_SQ, {"fleet": "Sit tight and dodge with everything you've got."},
     [goal("all", "hold"), c("all", "dodge", "all", gte=3), c("Vigilant", "dodge", "all", gte=6, what="Vigilant dodges all 6 G")])
case("abc-hold-dodge", "dodge", ABC, "Blue", ABC_SQ, {"fleet": "Don't advance. Hold here, and keep at least 3G for dodging."},
     [goal("all", "hold"), c("all", "dodge", "all", gte=3), eng_free("all")])

# --- ship-level overrides
case("fs-hesperus-escape", "ship", FS, "Blue", FS_SQ, {"fleet": "Destroy the enemy.", "ship": {"CS Hesperus": "Get out of here: jump."}},
     [{"own_script": "CS Hesperus"}, c("ship:CS Hesperus", "goal", "any", eq="escape"), goal("Lance", "destroy")])
case("abc-osprey-hold", "ship", ABC, "Blue", ABC_SQ,
     {"squadron": {"Corvettes": "Engage their fighters."}, "ship": {"IMS Osprey": "Stay back and hold your position."}},
     [{"own_script": "IMS Osprey"}, goal("ship:IMS Osprey", "hold"), eng_fighters("Corvettes"), {"no_own_script": "IMS Lark"}])
case("fs-lance07-tarsis", "ship", FS, "Blue", FS_SQ, {"ship": {"Lance 07": "Burn toward CS Tarsis at 6G."}},
     [{"own_script": "Lance 07"}, c("ship:Lance 07", "movement.mode", eq="burn"), c("ship:Lance 07", "movement.toward.ship", eq="CS Tarsis"),
      c("ship:Lance 07", "movement.thrust", eq=6), c("Lance", "movement.mode", "all", eq="network", what="rest of Lance unchanged"),
      {"no_own_script": "Lance 01"}])
case("cv-anselm-jump", "ship", CV, "Blue", CV_SQ,
     {"squadron": {"Convoy": "Stay with the escorts."}, "ship": {"Void Trader Anselm": "Jump out now."}},
     [{"own_script": "Void Trader Anselm"}, c("ship:Void Trader Anselm", "goal", eq="escape"),
      c("Convoy", "goal", "all", eq="escape", not_=True, what="rest of Convoy stay"), {"no_own_script": "Void Trader Bellweather"}])
case("fs-lance01-cruiser", "ship", FS, "Blue", FS_SQ, {"ship": {"Lance 01": "Go after CS Tarsis."}},
     [{"own_script": "Lance 01"},
      any_of(c("ship:Lance 01", "engage.ships", contains="CS Tarsis"), c("ship:Lance 01", "engage.designs", contains=KIN),
             what="Lance 01 engages Tarsis"),
      eng_free("Lance")])

# --- layered fleet + squadron + ship
case("fs-layered-2d", "layered", FS, "Blue", FS_SQ,
     {"fleet": "Hold your ground and let them come to us.", "squadron": {"Lance": "Keep their fighters off the cruiser."}},
     [goal("Lance", "hold"), eng_fighters("Lance"), goal("Flag", "hold"), eng_free("Flag")])
case("abc-boats-override", "layered", ABC, "Blue", ABC_SQ,
     {"fleet": "All squadrons hold position.", "squadron": {"Boats": "Ignore the fleet order: go and attack their warships."}},
     [goal("Boats", "destroy"), eng_design("Boats", PW, alt=[c("Boats", "engage.min_tons", gte=11)], what="Boats engage their warships (not fighters)"),
      goal("Escorts", "hold"), goal("Corvettes", "hold")])
case("abc-three-layers", "layered", ABC, "Blue", ABC_SQ,
     {"fleet": "Engage their fighters first.", "squadron": {"Escorts": "Keep 3G for dodging."}, "ship": {"IMS Valiant": "Hold position."}},
     [eng_fighters("Corvettes"), eng_fighters("Escorts"), c("Escorts", "dodge", "all", gte=3), {"own_script": "IMS Valiant"},
      goal("ship:IMS Valiant", "hold"), c("ship:IMS Valiant", "dodge", "all", gte=3, what="Valiant keeps her squadron's dodge"),
      eng_fighters("ship:IMS Valiant")])
case("swr-layered", "layered", SW, "Red", SWR_SQ, {"fleet": "Go for the destroyer escort.", "squadron": {"Bravo": "Take the cruiser instead."}},
     [eng_design("Alpha", DE, alt=[c("Alpha", "engage.ships", contains="IMS Vigilant")]),
      eng_design("Bravo", MC, alt=[c("Bravo", "engage.ships", contains="Iron Hand")]),
      c("Bravo", "engage.ships", contains="IMS Vigilant", not_=True, what="Bravo not on Vigilant")])
case("sw-protect", "layered", SW, "Blue", SW_SQ,
     {"fleet": "Iron Hand, use your missiles on the fighters. Vigilant, stay close to Iron Hand and protect her."},
     [any_of(all_of(goal("ship:IMS Vigilant", "hold"),
                    any_of(c("ship:IMS Vigilant", "post.ship", eq="Iron Hand"), c("ship:IMS Vigilant", "post.squadron", eq="Hand"))),
             c("ship:IMS Vigilant", "movement.toward.ship", eq="Iron Hand"), c("ship:IMS Vigilant", "movement.at.ship", eq="Iron Hand"),
             what="Vigilant stays on Iron Hand"),
      c("ship:Iron Hand", "goal", "all", in_=["destroy", "hold"], what="Iron Hand fights")])

# --- ambiguous: the least-change reading
case("fs-careful", "ambiguous", FS, "Blue", FS_SQ, {"squadron": {"Lance": "Be careful out there."}},
     [goal("Lance", "destroy"), eng_free("Lance"), c("Lance", "movement.mode", "all", eq="network", what="no set movement"), eng_free("Flag")])
case("fs-press", "ambiguous", FS, "Blue", FS_SQ, {"fleet": "Press them."},
     [goal("all", "destroy"), eng_free("all")])
case("abc-support", "ambiguous", ABC, "Blue", ABC_SQ, {"squadron": {"Corvettes": "Support the escorts."}},
     [eng_free("Corvettes"), c("Corvettes", "goal", "all", in_=["destroy", "hold"], what="Corvettes fight"), eng_free("Escorts"),
      goal("Escorts", "destroy")])
case("pk-dont-let-through", "ambiguous", PK, "Blue", PK_SQ, {"fleet": "Don't let them through."},
     [goal("all", "hold"), eng_free("all")])

# --- cannot be expressed: say so, do the nearest thing
case("fs-board", "inexpressible", FS, "Blue", FS_SQ, {"ship": {"Lance 03": "Board CS Tarsis."}},
     [{"say_any": CANT, "what": "say explains"},
      any_of(c("ship:Lance 03", "engage.ships", contains="CS Tarsis"), c("ship:Lance 03", "movement.at.ship", eq="CS Tarsis"),
             c("ship:Lance 03", "movement.toward.ship", eq="CS Tarsis"), c("ship:Lance 03", "engage.designs", contains=KIN),
             what="nearest thing: go for Tarsis")])
case("fs-wedge", "inexpressible", FS, "Blue", FS_SQ, {"fleet": "Form a wedge with the cruiser at the point."},
     [{"say_any": CANT + ["formation", "wedge"], "what": "say explains"}, eng_free("all")])
case("tb-executor-unseen", "inexpressible", TB, "Blue", {}, {"ship": {"Invigilator": "Close to Short with HMS Executor and cripple her drive."}},
     [{"say_any": CANT + ["not yet", "no contact", "not found", "not detected", "unidentified", "not on", "isn't", "is not",
                          "blip", "unseen", "not seen", "unknown"], "what": "say explains"},
      {"call_me_when": "new_enemies", "what": "asks to hear when she is found"}])
case("fs-take-alive", "inexpressible", FS, "Blue", FS_SQ,
     {"squadron": {"Lance": "We want CS Tarsis taken alive: stop firing once her drive is out."}},
     [{"say_any": CANT, "what": "say explains"},
      any_of(c("Lance", "engage.ships", contains="CS Tarsis"), c("Lance", "engage.designs", contains=KIN), what="Lance on Tarsis")])
case("abc-until-dead", "inexpressible", ABC, "Blue", ABC_SQ,
     {"squadron": {"Escorts": "Engage Ftahkoa until she is destroyed, then go after their fighters."}},
     [c("Escorts", "engage.ships", contains="Ftahkoa", what="Escorts first on Ftahkoa"),
      any_of({"say_any": CANT}, eng_fighters("Escorts", "last"), what="fighters next, or said why not")])

# --- withdrawals
case("abc-red-withdraw", "withdraw", ABC, "Red", ABCR_SQ, {"fleet": "Withdraw. Everyone jump out."},
     [c("all", "goal", "any", eq="escape", what="all escape")])
case("cv-escape-cover", "withdraw", CV, "Blue", CV_SQ,
     {"squadron": {"Convoy": "Jump out as soon as you can.", "Escort": "Cover the merchants until they are gone, then jump yourselves."}},
     [goal("Convoy", "escape", "first"), c("Escort", "goal", "first", in_=["hold", "destroy"], what="Escort covers first"),
      # No end can test "they are gone": a timed jump, or saying so, will do.
      any_of(c("Escort", "goal", "last", eq="escape"), {"say_any": CANT}, what="Escort then escapes, or says why not")])
case("fs-half-then-jump", "withdraw", FS, "Blue", FS_SQ, {"fleet": "Any squadron that loses half its hull: pull out and jump."},
     [steps_gte("all", 2), c("all", "until.losses", eq=0.5), goal("all", "escape", "last"), goal("all", "destroy", "first")])
case("pk-fall-back", "withdraw", PK, "Blue", PK_SQ, {"fleet": "Fall back to Kherra and hold there."},
     [any_of(c("all", "post.planet", "any", eq="Kherra"), c("all", "movement.at.planet", "any", eq="Kherra"),
             c("all", "movement.toward.planet", "any", eq="Kherra"), what="to Kherra"),
      goal("all", "hold", "last"), c("all", "goal", "all", eq="escape", not_=True, what="no escape")])

# --- tricky phrasings
case("fs-user-hold-dodge", "tricky", FS, "Blue", FS_SQ,
     {"fleet": "Hold, engaging their fighters as they come in range. Put at least 3G into dodge."},
     [goal("all", "hold"), c("all", "dodge", "all", gte=3), eng_fighters("Lance"),
      c("all", "movement.mode", "all", eq="network", what="no set movement")])
case("fs-user-tarsis-medium", "tricky", FS, "Blue", FS_SQ,
     {"squadron": {"Flag": "Close to Medium range with CS Tarsis while firing on Tarsis."}},
     [any_of(c("Flag", "engage.ships", contains="CS Tarsis"), c("Flag", "engage.designs", contains=KIN), what="Flag fires on Tarsis"),
      any_of(all_of(c("Flag", "movement.mode", eq="arrive"), c("Flag", "movement.at.ship", eq="CS Tarsis"), c("Flag", "movement.band", eq="Medium")),
             all_of(c("Flag", "movement.mode", eq="burn"), c("Flag", "movement.toward.ship", eq="CS Tarsis"),
                    c("Flag", "until.within.band", eq="Medium")),
             what="closes to Medium of Tarsis"),
      eng_free("Lance")])
case("fs-call-me", "tricky", FS, "Blue", FS_SQ,
     {"fleet": "Fight as you see fit. Call me if Lance loses a quarter of its strength or if anything new shows up."},
     [{"call_me_when": "losses"}, {"call_me_when": "new_enemies"}, eng_free("all"), {"no_call_me_when": "round", "what": "no round trigger"}])
case("abc-boats-hang-back", "tricky", ABC, "Blue", ABC_SQ,
     {"squadron": {"Boats": "Keep three gees on dodge and hang back at Long range from their main body."}},
     [c("Boats", "dodge", "all", eq=3),
      any_of(c("Boats", "movement.band", "any", eq="Long"), c("Boats", "until.within.band", "any", eq="Long"), what="Long range")])
case("fs-flag-out-of-reach", "tricky", FS, "Blue", FS_SQ, {"ship": {"CS Hesperus": "Stay out of reach of their fighters' guns."}},
     [{"own_script": "CS Hesperus"},
      any_of(c("ship:CS Hesperus", "movement.band", "any", in_=["VeryLong", "Distant"]), c("ship:CS Hesperus", "movement.away", "any", eq=True),
             what="beyond Long of the fighters"),
      eng_free("Lance")])
case("fs-hold-till-3", "tricky", FS, "Blue", FS_SQ, {"fleet": "Hold until round 3, then everyone attack."},
     [goal("all", "hold", "first"), c("all", "until.round", eq=3), goal("all", "destroy", "last")])
case("abc-red-iyre-swarm", "tricky", ABC, "Red", ABCR_SQ,
     {"squadron": {"Iyre": "Swarm the nearest corvette and finish it before moving on."}},
     [any_of(c("Iyre", "engage.designs", contains=PC), c("Iyre", "engage.ships", in_=["IMS Lark", "IMS Merlin", "IMS Harrier", "IMS Osprey"]),
             c("Iyre", "engage.ships", contains="IMS Lark"), c("Iyre", "engage.ships", contains="IMS Merlin"),
             c("Iyre", "engage.ships", contains="IMS Harrier"), c("Iyre", "engage.ships", contains="IMS Osprey"), what="Iyre on a corvette"),
      eng_free("Warships"), eng_free("Escorts")])
case("abc-red-tonnage", "tricky", ABC, "Red", ABCR_SQ,
     {"fleet": "Leave anything under 500 tons to the fighters; the big ships take the rest."},
     [any_of(c("Iyre", "engage.max_tons", lte=500), c("Iyre", "engage.designs", contains=PC), what="Iyre on the small ones"),
      any_of(c("Warships", "engage.min_tons", gte=401), c("Warships", "engage.designs", contains=DE), what="Warships on the big"),
      any_of(c("Escorts", "engage.min_tons", gte=401), c("Escorts", "engage.designs", contains=DE), what="Escorts on the big")])

# --- carriers: flight operations
case("kh-hold-then-launch", "flight", KH, "Blue", KH_SQ,
     {"fleet": "Keep the fighters aboard until the cruisers are fired on, then launch everything."},
     [c("Battle Line", "flight.op", "first", eq="hold", what="first hold the craft"),
      c("Battle Line", "until.fired_on", "first", eq=True, what="until fired on"),
      c("Battle Line", "flight.op", "last", eq="launch", what="then launch"), steps_gte("Battle Line", 2)])
case("kh-launch-now", "flight", KH, "Blue", KH_SQ, {"squadron": {"Battle Line": "Sirocco, launch all your fighters now."}},
     [c("Battle Line", "flight.op", "first", eq="launch", what="launch"),
      c("Battle Line", "flight.count", "first", absent=True, what="all of them"),
      c("Screen", "flight", "all", absent=True, what="Screen has no flight order")])
case("kh-launch-forty", "flight", KH, "Blue", KH_SQ, {"squadron": {"Battle Line": "Put forty fighters out and keep the rest aboard."}},
     [c("Battle Line", "flight.op", "first", eq="launch"), c("Battle Line", "flight.count", "first", eq=40, what="forty")])
case("kh-recover-and-jump", "flight", KH, "Blue", KH_SQ, {"fleet": "Recover the fighters, then jump out."},
     [c("Battle Line", "flight.op", "first", eq="recover", what="recover first"), goal("Battle Line", "escape", "last"),
      steps_gte("Battle Line", 2)])
case("kh-crews-run-deck", "flight", KH, "Blue", KH_SQ, {"fleet": "Attack. Destroy the depot's defenders."},
     [goal("all", "destroy"), eng_free("all"), c("Battle Line", "flight", "all", absent=True, what="the crews run the deck")])

print(len(cases))
with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "admiral_orders.json"), "w") as f:
    f.write("[\n" + ",\n".join(json.dumps(cs, ensure_ascii=False) for cs in cases) + "\n]\n")
