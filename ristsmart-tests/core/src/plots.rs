use anyhow::Result;
use plotters::prelude::*;
use plotters_svg::SVGBackend;
use crate::metrics::RunContext;

pub fn render_all(ctx: &RunContext, outdir: &std::path::Path) -> Result<()> {
    plot_total_tp(ctx, &outdir.join("throughput_total.png"))?;
    plot_total_tp_svg(ctx, &outdir.join("throughput_total.svg"))?;
    plot_weights_overlay(ctx, &outdir.join("weights_overlay.png"))?;
    plot_weights_overlay_svg(ctx, &outdir.join("weights_overlay.svg"))?;
    plot_per_link_throughput(ctx, &outdir.join("per_link_throughput.png"))?;
    
    // Save metrics as JSON and CSV
    ctx.metrics.save_json(&outdir.join("metrics.json"))?;
    ctx.metrics.save_csv(&outdir.join("metrics.csv"))?;
    
    Ok(())
}

fn plot_total_tp(ctx: &RunContext, path: &std::path::Path) -> Result<()> {
    let root = BitMapBackend::new(path, (1280, 720)).into_drawing_area();
    root.fill(&WHITE)?;
    draw_tp(ctx, &root)
}

fn plot_total_tp_svg(ctx: &RunContext, path: &std::path::Path) -> Result<()> {
    let root = SVGBackend::new(path, (1280, 720)).into_drawing_area();
    root.fill(&WHITE)?;
    draw_tp(ctx, &root)
}

fn draw_tp<DB: DrawingBackend>(ctx: &RunContext, area: &DrawingArea<DB, plotters::coord::Shift>) -> Result<()> 
where 
    <DB as plotters::prelude::DrawingBackend>::ErrorType: 'static 
{
    if ctx.metrics.samples.is_empty() {
        return Ok(());
    }

    let xs: Vec<i64> = ctx.metrics.samples.iter().map(|s| s.ts_ms as i64).collect();
    let achieved: Vec<f64> = ctx.metrics.samples.iter().map(|s| s.achieved_bps).collect();
    let theoretical: Vec<f64> = ctx.metrics.samples.iter().map(|s| s.theoretical_bps).collect();

    let x_min = *xs.first().unwrap_or(&0);
    let x_max = *xs.last().unwrap_or(&1);
    let y_max = achieved.iter().chain(theoretical.iter()).cloned().fold(1.0, f64::max);

    let mut chart = ChartBuilder::on(area)
        .caption("Total throughput (payload bps): achieved vs theoretical", ("sans-serif", 24))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(80)
        .build_cartesian_2d(x_min..x_max, 0.0..y_max * 1.1)?;

    chart.configure_mesh()
        .x_desc("Time (ms)")
        .y_desc("Throughput (bps)")
        .label_style(("sans-serif", 16))
        .draw()?;

    chart.draw_series(LineSeries::new(
        xs.iter().cloned().zip(achieved.iter().cloned()), 
        &BLUE
    ))?
    .label("Achieved")
    .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 30, y)], &BLUE));

    chart.draw_series(LineSeries::new(
        xs.iter().cloned().zip(theoretical.iter().cloned()), 
        &RED
    ))?
    .label("Theoretical")
    .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 30, y)], &RED));

    chart.configure_series_labels()
        .border_style(&BLACK)
        .draw()?;
    
    Ok(())
}

fn plot_weights_overlay(ctx: &RunContext, path: &std::path::Path) -> Result<()> {
    let root = BitMapBackend::new(path, (1280, 720)).into_drawing_area();
    root.fill(&WHITE)?;
    draw_weights(ctx, &root)
}

fn plot_weights_overlay_svg(ctx: &RunContext, path: &std::path::Path) -> Result<()> {
    let root = SVGBackend::new(path, (1280, 720)).into_drawing_area();
    root.fill(&WHITE)?;
    draw_weights(ctx, &root)
}

fn draw_weights<DB: DrawingBackend>(ctx: &RunContext, area: &DrawingArea<DB, plotters::coord::Shift>) -> Result<()> 
where 
    <DB as plotters::prelude::DrawingBackend>::ErrorType: 'static 
{
    if ctx.metrics.samples.is_empty() {
        return Ok(());
    }

    let xs: Vec<i64> = ctx.metrics.samples.iter().map(|s| s.ts_ms as i64).collect();
    let links = ctx.links;

    let mut chart = ChartBuilder::on(area)
        .caption("Dispatcher weights vs ideal weights", ("sans-serif", 24))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(80)
        .build_cartesian_2d(
            *xs.first().unwrap_or(&0)..*xs.last().unwrap_or(&1), 
            0.0..1.0
        )?;

    chart.configure_mesh()
        .x_desc("Time (ms)")
        .y_desc("Weight")
        .label_style(("sans-serif", 16))
        .draw()?;

    let colors = [&BLUE, &RED, &GREEN, &MAGENTA, &CYAN, &BLACK];
    
    for i in 0..links {
        // Dispatcher weights
        let disp: Vec<(i64, f64)> = ctx.metrics.samples.iter().filter_map(|s| {
            let w = s.dispatcher_weights.as_ref()?.get(i).copied()?;
            Some((s.ts_ms as i64, w))
        }).collect();
        
        if !disp.is_empty() {
            let c = colors[i % colors.len()];
            chart.draw_series(LineSeries::new(disp.clone(), c.stroke_width(2)))?
                .label(format!("Link {} dispatcher", i))
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 30, y)], c.stroke_width(2)));
        }
        
        // Ideal weights (dashed line style)
        let ideal: Vec<(i64, f64)> = ctx.metrics.samples.iter().filter_map(|s| {
            let w = s.ideal_weights.as_ref()?.get(i).copied()?;
            Some((s.ts_ms as i64, w))
        }).collect();
        
        if !ideal.is_empty() {
            let c = colors[i % colors.len()];
            // Draw with dashed style (approximate by drawing points)
            chart.draw_series(
                ideal.iter()
                    .step_by(3) // Skip points to simulate dashing
                    .map(|(x, y)| Circle::new((*x, *y), 2, c.filled()))
            )?
            .label(format!("Link {} ideal", i))
            .legend(move |(x, y)| Circle::new((x + 15, y), 3, c.filled()));
        }
    }

    chart.configure_series_labels()
        .border_style(&BLACK)
        .draw()?;
    
    Ok(())
}

fn plot_per_link_throughput(ctx: &RunContext, path: &std::path::Path) -> Result<()> {
    if ctx.metrics.samples.is_empty() {
        return Ok(());
    }

    let root = BitMapBackend::new(path, (1280, 720)).into_drawing_area();
    root.fill(&WHITE)?;

    let xs: Vec<i64> = ctx.metrics.samples.iter().map(|s| s.ts_ms as i64).collect();
    let links = ctx.links;

    // Calculate per-link throughput from byte deltas
    let mut per_link_throughput: Vec<Vec<f64>> = vec![Vec::new(); links];
    
    for (i, sample) in ctx.metrics.samples.iter().enumerate() {
        if i > 0 {
            let prev_sample = &ctx.metrics.samples[i - 1];
            let dt_s = (sample.ts_ms - prev_sample.ts_ms) as f64 / 1000.0;
            
            if dt_s > 0.0 {
                for link_idx in 0..links {
                    if let (Some(curr), Some(prev)) = (
                        sample.link_bytes.get(link_idx),
                        prev_sample.link_bytes.get(link_idx)
                    ) {
                        let bytes_delta = curr.saturating_sub(*prev);
                        let bps = (bytes_delta as f64 * 8.0) / dt_s;
                        per_link_throughput[link_idx].push(bps);
                    } else {
                        per_link_throughput[link_idx].push(0.0);
                    }
                }
            }
        }
    }

    // Skip first sample since we can't compute delta
    let xs_delta: Vec<i64> = xs.iter().skip(1).cloned().collect();
    let y_max = per_link_throughput.iter()
        .flat_map(|v| v.iter())
        .cloned()
        .fold(1.0, f64::max);

    let mut chart = ChartBuilder::on(&root)
        .caption("Per-link throughput", ("sans-serif", 24))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(80)
        .build_cartesian_2d(
            *xs_delta.first().unwrap_or(&0)..*xs_delta.last().unwrap_or(&1), 
            0.0..y_max * 1.1
        )?;

    chart.configure_mesh()
        .x_desc("Time (ms)")
        .y_desc("Throughput (bps)")
        .label_style(("sans-serif", 16))
        .draw()?;

    let colors = [&BLUE, &RED, &GREEN, &MAGENTA, &CYAN, &BLACK];
    
    for (link_idx, throughput) in per_link_throughput.iter().enumerate() {
        if !throughput.is_empty() && throughput.len() == xs_delta.len() {
            let c = colors[link_idx % colors.len()];
            chart.draw_series(LineSeries::new(
                xs_delta.iter().cloned().zip(throughput.iter().cloned()), 
                c.stroke_width(2)
            ))?
            .label(format!("Link {}", link_idx))
            .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 30, y)], c.stroke_width(2)));
        }
    }

    chart.configure_series_labels()
        .border_style(&BLACK)
        .draw()?;
    
    Ok(())
}
