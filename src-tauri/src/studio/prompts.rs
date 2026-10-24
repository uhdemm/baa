//! What we ask the image model for. One approved master first, then one key-pose
//! sheet per mood with the master attached as the identity reference: the
//! "one master, then motion" and "strip-first" rules from Sprite Studio
//! (github.com/JohnKinyanjui/sprite-maker, MIT), tuned for a buddy that stands
//! on the taskbar at about 75 px tall.

/// One mood sheet: what it shows and the six key poses, in order.
pub struct Mood {
    pub id: &'static str,
    pub what: &'static str,
    /// lip sync: everything but the mouth is traced from frame 1
    pub lipsync: bool,
    pub keys: [&'static str; 6],
}

pub const MOODS: &[Mood] = &[
    Mood {
        id: "idle",
        what: "standing around idle, alive but calm (a loop)",
        lipsync: false,
        keys: [
            "REST: the master pose, standing relaxed, small smile.",
            "BREATHE IN: same pose, body very slightly taller and rounder.",
            "BLINK: same as REST with the eyes closed.",
            "LOOK: same pose, eyes and head glancing to its NEAR side.",
            "SWAY: weight shifted onto the FAR foot, tiny happy bounce.",
            "SCRATCH: one hoof scratching the side of its head, eyes half shut.",
        ],
    },
    Mood {
        id: "talk",
        what: "talking: a lip-sync mouth chart, attentive and looking at the viewer",
        lipsync: true,
        keys: [
            "CLOSED: mouth shut in a small calm line.",
            "SMALL: mouth slightly open, a thin dark gap.",
            "OPEN: mouth open medium, tongue just visible, like 'ah'.",
            "WIDE: mouth open wide and round, like a loud 'BAA'.",
            "OO: lips pushed forward into a small round 'oo'.",
            "EE: mouth stretched wide and flat showing teeth, like 'ee'.",
        ],
    },
    Mood {
        id: "think",
        what: "thinking hard while it works something out (a loop)",
        lipsync: false,
        keys: [
            "HMM: one hoof on its chin, eyes looking up.",
            "HMM 2: same, eyes looking up to the other side, mouth pursed.",
            "TAP: tapping its temple with one hoof, tongue poking out of the corner of its mouth.",
            "TAP 2: same with the hoof lifted a little off the head.",
            "AHA: eyes wide, one hoof raised with a pointing idea gesture, mouth in a little 'oh!'.",
            "GOT IT: confident grin, both arms back down.",
        ],
    },
    Mood {
        id: "send",
        what: "sending a message: it winds up and throws a paper airplane forward, then celebrates",
        lipsync: false,
        keys: [
            "READY: holding a small white paper airplane up in its NEAR hoof, looking determined.",
            "WIND UP: leaning back, NEAR arm cocked behind its head with the airplane.",
            "THROW: lunging forward, NEAR arm extended forward, the airplane just leaving the hoof.",
            "FOLLOW THROUGH: arm across the body, airplane gone off the edge of the cell, eyes following it.",
            "YES: both arms up in a small cheer, big open smile.",
            "SETTLE: back to standing, proud little smile.",
        ],
    },
    Mood {
        id: "error",
        what: "something went wrong: grumpy and flustered, then sulking",
        lipsync: false,
        keys: [
            "HUH: confused frown, one ear up, one down.",
            "GRR: stomping the NEAR foot, cheeks puffed, angry eyebrows.",
            "STOMP: the other foot stomping, fists clenched.",
            "SIGH: shoulders dropped, eyes closed, big sigh.",
            "ARMS CROSSED: arms folded, looking away, sulking.",
            "OKAY: shrugging with both hooves up, wobbly smile.",
        ],
    },
    Mood {
        id: "sleep",
        what: "dozing off where it stands and waking up",
        lipsync: false,
        keys: [
            "YAWN: stretching both arms up in a huge yawn, eyes shut.",
            "SIT: sitting down on its bottom, eyelids heavy.",
            "ASLEEP: sitting, head drooped forward, eyes shut, mouth a little open.",
            "SNORE: same as asleep with cheeks puffed mid-snore and ears flopped down.",
            "STARTLE: jolted awake and back on its feet, eyes huge, ears straight up.",
            "READY: standing, alert and smiling at the viewer.",
        ],
    },
    Mood {
        id: "appear",
        what: "popping up to say hi: it hops in, lands, and waves",
        lipsync: false,
        keys: [
            "CROUCH: squashed down low, about to jump, knees bent.",
            "HOP: in the air, body stretched tall, arms up.",
            "LAND: landing with knees bent, body squashed a little.",
            "STAND: standing up straight, eyes bright.",
            "WAVE: waving with the NEAR hoof, big smile.",
            "WAVE 2: same wave with the hoof at the other end of the swing.",
        ],
    },
];

/// The master pose. `has_refs`: the user attached pictures of the character or the
/// style they want; those win over everything except the small-size rules.
pub fn master_prompt(description: &str, has_refs: bool, out: &str) -> String {
    let refs = if has_refs {
        "Use the attached images as the EXACT character and drawing style (line quality, texture, proportions, \
colours, personality). Do not simplify it into a generic cartoon.\n\n"
    } else {
        ""
    };
    format!(
        "{refs}Design a character for a voice-typing app called Baa: {description}\n\n\
Draw it in whatever style the description asks for. It will be shown SMALL on a computer screen (about 75 \
pixels tall, standing on the taskbar, a speech bubble next to it), so whatever the style:\n\
- the outer silhouette is clear and a little bolder than the inner lines\n\
- big readable eyes and a clear mouth area, because it will lip-sync\n\
- body areas are filled solid (no see-through fills); only the empty background is transparent\n\
- if it has two arms and two legs, the FAR arm and FAR leg are a slightly darker shade than the NEAR ones\n\n\
Pose: full body, standing, facing the viewer at a slight three-quarter angle, arms relaxed, mouth closed, \
feet on the ground. Show the WHOLE body from the top of the head to the bottom of the feet, centred, with \
clear empty space all around; nothing cut off.\n\n\
Transparent background (PNG with alpha): no paper, no background colour, no ground, no shadow, no text. Save to {out}"
    )
}

pub fn sheet_prompt(m: &Mood, out: &str) -> String {
    let keys = m.keys.iter().enumerate().map(|(i, k)| format!("{}. {k}", i + 1)).collect::<Vec<_>>().join("\n");
    let same = if m.lipsync {
        "\nIMPORTANT: this is for lip sync, so the head, eyes, ears, body, arms and legs are IDENTICAL in all 6 \
frames, traced from frame 1 in exactly the same position. ONLY THE MOUTH changes.\n"
    } else {
        ""
    };
    format!(
        "Use the attached image as the exact character: same design, colours, proportions, line quality and drawing style. \
Keep its identity in every frame (no new features, no colour drift, no change of proportions).\n\n\
Make an ANIMATION KEY-POSE SHEET of 6 frames of it {what}, in a 3 x 2 grid (3 frames on top, 3 below, read \
left to right).\n\n\
Every frame: the SAME size and scale as the attached image, full body from head to feet, feet standing on the \
same invisible ground line at the bottom of each cell (a jump goes up from that line), facing the same way. \
Name limbs by depth: the NEAR arm/leg is closer to the camera, the FAR one is behind it and keeps its slightly \
darker shade in every frame. Nothing touches or crosses the edge of its cell, and clear empty space separates \
the cells so no frame touches another. Filled areas (fur, wool, clothes) stay solid and opaque, never \
see-through.\n{same}\n{keys}\n\n\
Transparent background (PNG with alpha): no background colour, no shadows, no ground line, no grid lines, no \
frame numbers, no text. Save to {out}",
        what = m.what
    )
}
