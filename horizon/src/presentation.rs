use crate::{
    Mod,
    model::{Flag, Volume},
    store::Particle,
};
use mod_api::{ParticleOptions, PlayerRef, ServerHost, Text, text::TextStyle};

pub fn occupation(host: &mut dyn ServerHost, player: PlayerRef, owner: &str) {
    let title = colored("私人领地", "gold");
    let subtitle = colored("已被 ", "gray")
        .append(colored(owner, "white"))
        .append(colored(" 占领.", "gray"));
    let _ = player.title(host, &title, &subtitle, [10, 40, 10]);
}

pub fn wilderness(host: &mut dyn ServerHost, player: PlayerRef) {
    let title = colored("荒地", "black");
    let subtitle = colored("无人占领", "gray");
    let _ = player.title(host, &title, &subtitle, [10, 40, 10]);
}

fn colored(value: &str, color: &str) -> Text {
    let mut style = TextStyle::default();
    style.color = Some(color.into());
    Text::literal(value).with_style(style)
}

pub fn draw_boundaries(modification: &Mod, host: &mut dyn ServerHost) {
    let Some(particle) = modification.boundary_particle.get() else {
        return;
    };
    let period = {
        let service = modification.service();
        let Ok(config) = service.ready() else { return };
        config.settings.particle.period.max(1)
    };
    if !modification.service().ticks.is_multiple_of(period) {
        return;
    }

    let options = ParticleOptions::simple(*particle);
    for player in host.players().server_players() {
        let Some((dimension, viewer)) = player.dimension(host).zip(player.entity().position(host))
        else {
            continue;
        };
        let dimension = dimension.to_string();
        let points = {
            let service = modification.service();
            let Ok(config) = service.ready() else { return };
            let world = service.world(&dimension);
            let volumes = service
                .index
                .areas
                .values()
                .filter(|area| {
                    area.world == world
                        && area.enabled(Flag::Particle, Flag::Particle.default_value())
                })
                .flat_map(|area| &area.volumes);
            boundary_points(volumes, viewer, &config.settings.particle)
        };
        for at in points {
            let _ = player.particle(host, &options, None, at);
        }
    }
}

pub fn boundary_points<'a>(
    volumes: impl Iterator<Item = &'a Volume>,
    viewer: [f64; 3],
    config: &Particle,
) -> Vec<[f64; 3]> {
    let range = config.range.clamp(1.0, 128.0);
    let distance = config.distance.max(0.5);
    let budget = config.budget.min(2000);
    let within = |pos: [f64; 3], radius: f64| {
        pos.into_iter()
            .zip(viewer)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            <= radius.powi(2)
    };
    let mut points = Vec::new();
    if !range.is_finite() || !distance.is_finite() || budget == 0 {
        return points;
    }
    for volume in volumes {
        let horizontal_radius = [0, 2]
            .into_iter()
            .flat_map(|axis| {
                [
                    volume.max[axis] - volume.core_pos[axis],
                    volume.core_pos[axis] - volume.min[axis],
                ]
            })
            .max()
            .unwrap_or(0);
        if !within(
            volume.core_pos.map(f64::from),
            range + f64::from(horizontal_radius),
        ) {
            continue;
        }
        let min = volume.min.map(f64::from);
        let max = volume.max.map(|value| f64::from(value) + 1.0);
        for axis in 0..3 {
            for fixed_1 in [min[(axis + 1) % 3], max[(axis + 1) % 3]] {
                for fixed_2 in [min[(axis + 2) % 3], max[(axis + 2) % 3]] {
                    let mut pos = min;
                    pos[(axis + 1) % 3] = fixed_1;
                    pos[(axis + 2) % 3] = fixed_2;
                    while pos[axis] <= max[axis] {
                        if within(pos, range) {
                            points.push(pos);
                            if points.len() == budget {
                                return points;
                            }
                        }
                        pos[axis] += distance;
                    }
                }
            }
        }
    }
    points
}
