//! realistic 真实感风格族。

pub mod cinematic;
pub mod documentary;
pub mod vlog;

use crate::models::StyleProfile;

/// Shared negative prompt for photorealistic human video generation.
/// The positive motion footer adds the corresponding continuity instructions.
pub const REALISTIC_NEGATIVE_BASE: &str = "text, letters, subtitles, captions, Chinese characters, English words, numbers, watermark, logo, signature, border frame, cartoon, illustration, anime, 3D render, CGI artifacts, distorted faces, asymmetrical face, extra limbs, extra arms, extra hands, missing fingers, malformed hands, mutated hands, twisted body, impossible anatomy, 360-degree head rotation, unnatural joint rotation, flickering, jitter, camera shake, frame instability, flicker, morphing, warping, deformation, low quality";

fn compose_negative(extra: &str) -> String {
    format!("{REALISTIC_NEGATIVE_BASE}, {extra}")
}

pub fn profiles() -> Vec<StyleProfile> {
    vec![
        cinematic::profile(),
        vlog::profile(),
        documentary::profile(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_prompt_contains_human_safety_constraints() {
        let profile = cinematic::profile();
        assert!(profile.negative.contains("extra arms"));
        assert!(profile.negative.contains("360-degree head rotation"));
        assert!(profile.negative.contains("watermark"));
        assert!(profile.negative.contains("camera shake"));
        assert!(profile.motion_footer.contains("anatomically correct body"));
        assert!(profile.motion_footer.contains("stable locked-off motion"));
    }

    #[test]
    fn profiles_match_their_canvas_aspect_ratios() {
        assert_eq!(
            cinematic::profile().aspect_line(),
            "vertical 9:16 composition"
        );
        assert_eq!(vlog::profile().aspect_line(), "vertical 3:4 composition");
        assert_eq!(
            documentary::profile().aspect_line(),
            "horizontal 16:9 composition"
        );
    }

    #[test]
    fn style_header_and_prompt_have_three_parts() {
        let profile = cinematic::profile();
        let body = "a quiet street, morning light, slow tracking shot";
        let prompt = profile.build_prompt(body);
        let parts: Vec<&str> = prompt.split('\n').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1], body);
        assert_eq!(parts[2], profile.motion_footer);
        assert!(!profile.style_header().contains("{aspect}"));
    }
}
