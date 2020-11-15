extern crate clap;
extern crate v4l;

use std::io;
use std::time::Instant;

use clap::{App, Arg};
use v4l::io::stream::{Capture, Output};
use v4l::prelude::*;

fn main() -> io::Result<()> {
    let matches = App::new("v4l mmap")
        .version("0.2")
        .author("Christopher N. Hesse <raymanfx@gmail.com>")
        .about("Video4Linux forwarding example")
        .arg(
            Arg::with_name("device")
                .short("d")
                .long("device")
                .value_name("INDEX or PATH")
                .help("Device node path or index (default: 0)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("INDEX or PATH")
                .help("Device node path or index (default: 1)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("count")
                .short("c")
                .long("count")
                .value_name("INT")
                .help("Number of frames to capture (default: 4)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("buffers")
                .short("b")
                .long("buffers")
                .value_name("INT")
                .help("Number of buffers to allocate (default: 4)")
                .takes_value(true),
        )
        .get_matches();

    // Determine which device to use
    let mut source: String = matches
        .value_of("device")
        .unwrap_or("/dev/video0")
        .to_string();
    if source.parse::<u64>().is_ok() {
        source = format!("/dev/video{}", source);
    }
    println!("Using device: {}\n", source);

    // Determine which device to use
    let mut sink: String = matches
        .value_of("output")
        .unwrap_or("/dev/video1")
        .to_string();
    if sink.parse::<u64>().is_ok() {
        sink = format!("/dev/video{}", sink);
    }
    println!("Using sink device: {}\n", sink);

    // Capture 4 frames by default
    let count = matches.value_of("count").unwrap_or("4").to_string();
    let count = count.parse::<u32>().unwrap();

    // Allocate 4 buffers by default
    let buffers = matches.value_of("buffers").unwrap_or("4").to_string();
    let buffers = buffers.parse::<u32>().unwrap();

    let mut cap = CaptureDevice::with_path(source)?;
    println!("Active cap capabilities:\n{}", cap.query_caps()?);
    println!("Active cap format:\n{}", cap.format()?);
    println!("Active cap parameters:\n{}", cap.params()?);

    let mut out = OutputDevice::with_path(sink)?;
    println!("Active out capabilities:\n{}", out.query_caps()?);
    println!("Active out format:\n{}", out.format()?);
    println!("Active out parameters:\n{}", out.params()?);

    // BEWARE OF DRAGONS
    // Buggy drivers (such as v4l2loopback) only set the v4l2 buffer size (length field) once
    // a format is set, even though a valid format appears to be available when doing VIDIOC_G_FMT!
    // In our case, we just (try to) enforce the source format on the sink device.
    let source_fmt = cap.format()?;
    let sink_fmt = out.set_format(&source_fmt)?;
    if source_fmt.width != sink_fmt.width
        || source_fmt.height != sink_fmt.height
        || source_fmt.fourcc != sink_fmt.fourcc
    {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "failed to enforce source format on sink device",
        ));
    }
    println!("New out format:\n{}", out.format()?);

    // Setup a buffer stream and grab a frame, then print its data
    let mut cap_stream = MmapStream::with_buffers(&mut cap, buffers)?;

    let mut out_stream = MmapStream::with_buffers(&mut out, buffers)?;

    // warmup
    Capture::next(&mut cap_stream)?;

    let start = Instant::now();
    let mut megabytes_ps: f64 = 0.0;
    for i in 0..count {
        let t0 = Instant::now();
        let buf = Capture::next(&mut cap_stream)?;
        Output::next(&mut out_stream, buf.clone())?;
        let duration_us = t0.elapsed().as_micros();

        let cur = buf.len() as f64 / 1_048_576.0 * 1_000_000.0 / duration_us as f64;
        if i == 0 {
            megabytes_ps = cur;
        } else {
            // ignore the first measurement
            let prev = megabytes_ps * (i as f64 / (i + 1) as f64);
            let now = cur * (1.0 / (i + 1) as f64);
            megabytes_ps = prev + now;
        }

        println!("Buffer");
        println!("  sequence  : {}", buf.meta().sequence);
        println!("  timestamp : {}", buf.meta().timestamp);
        println!("  flags     : {}", buf.meta().flags);
        println!("  length    : {}", buf.len());
    }

    println!();
    println!("FPS: {}", count as f64 / start.elapsed().as_secs_f64());
    println!("MB/s: {}", megabytes_ps);

    Ok(())
}