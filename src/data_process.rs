use std::collections::HashMap;

use crate::acquisition::{do_acquisition, finer_doppler, AcquistionStatistics};
use crate::decoding::nav_decoding;
use crate::gps_constants;
use crate::tracking::{do_track, TrackingStatistics};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessStage {
    SignalAcquisition,
    FinerAcquisitionDoppler,
    SignalTracking,
    MessageDecoding,
}

pub fn do_data_process(
    data_in: &Vec<i16>,
    freq_sampling: f32,
    freq_IF: f32,
    stage: &mut ProcessStage,
    acquisition_statistic: &mut Vec<AcquistionStatistics>,
    tracking_statistic: &mut HashMap<i16, TrackingStatistics>,
    length_signal_ms: usize,
    is_complex: bool,
) {
    //let mut next_stage = stage;
    //let mut acq_statistic = acquire_statistic;
    //let mut tr_statistic = track_statistic;
    match stage {
        ProcessStage::SignalAcquisition => {
            if let Ok(()) = do_acquisition(data_in, acquisition_statistic, freq_sampling, freq_IF) {
                for acq_result in acquisition_statistic {
                    let mut tracking_result = &mut TrackingStatistics::new();
                    if let Some(result) = tracking_statistic.get_mut(&acq_result.prn) {
                        tracking_result = result;
                    };
                    tracking_result
                        .tracking_stat
                        .code_freq
                        .push(gps_constants::GPS_L1_CA_CODE_RATE_CHIPS_PER_S);
                    tracking_result.tracking_stat.code_phase_error.push(0.0);
                    tracking_result.tracking_stat.carrier_phase_error.push(0.0);
                    tracking_result.tracking_stat.code_error_filtered.push(0.0);
                    tracking_result.tracking_stat.code_error.push(0.0);
                    tracking_result
                        .tracking_stat
                        .carrier_error_filtered
                        .push(0.0);
                    tracking_result.tracking_stat.carrier_error.push(0.0);

                    let mut ca_code = acq_result.ca_code.clone();
                    ca_code.insert(
                        0,
                        ca_code[gps_constants::GPS_L1_CA_CODE_LENGTH_CHIPS as usize - 1],
                    );
                    ca_code.push(ca_code[1]);

                    let code_phase_step: f32 =
                        gps_constants::GPS_L1_CA_CODE_RATE_CHIPS_PER_S / freq_sampling;
                    let num_ca_code_samples = (gps_constants::GPS_L1_CA_CODE_LENGTH_CHIPS
                        / code_phase_step)
                        .ceil() as usize;
                    let ca_code_prompt: Vec<f32> = (0..num_ca_code_samples)
                        .map(|x| ca_code[(x as f32 * code_phase_step).ceil() as usize] as f32)
                        .collect();
                    tracking_result.ca_code_prompt.push(ca_code_prompt);
                }
                *stage = ProcessStage::FinerAcquisitionDoppler;
            } else {
            };
        }
        ProcessStage::FinerAcquisitionDoppler => {
            if let Some(()) = finer_doppler(
                data_in,
                length_signal_ms,
                is_complex,
                acquisition_statistic,
                freq_sampling,
                freq_IF,
            ) {
                for acq_result in acquisition_statistic {
                    let mut tracking_result = &mut TrackingStatistics::new();
                    if let Some(result) = tracking_statistic.get_mut(&acq_result.prn) {
                        tracking_result = result;
                    };
                    tracking_result
                        .tracking_stat
                        .carrier_freq
                        .push(acq_result.carrier_freq);
                }
                *stage = ProcessStage::SignalTracking;
            }
        }
        ProcessStage::SignalTracking => {
            if let Ok(()) = do_track(
                data_in,
                acquisition_statistic,
                tracking_statistic,
                freq_sampling,
                freq_IF,
            ) {
                *stage = ProcessStage::SignalTracking;
            } else {
                todo!(); // do tracking again with new data
            };
        }
        ProcessStage::MessageDecoding => {
            if let Ok(pos_result) = nav_decoding(tracking_statistic) {
            } else {
                todo!(); // do tracking again with new data
            }
        }
    }
}

mod test {
    use super::*;
    use crate::acquisition::do_acquisition;
    use crate::utilities::plot_samples;
    use binrw::BinReaderExt;
    use std::fs::File;
    use std::io::Read;
    use std::time::Instant;

    #[test]
    fn test_data_process() {
        let t1: Instant = Instant::now();
        let mut f = File::open("src/test_data/GPS_recordings/gioveAandB_short.bin")
            .expect("Error in opening file");
        let f_sampling: f32 = 16.3676e6;
        let f_inter_freq: f32 = 4.1304e6;

        let mut acq_results: Vec<AcquistionStatistics> = Vec::new();

        let mut tracking_statistic: HashMap<i16, TrackingStatistics> = HashMap::new();
        for i in 1..=32 {
            tracking_statistic.insert(i, TrackingStatistics::new());
        }

        let mut stage = ProcessStage::SignalAcquisition;
        let mut f_code = gps_constants::GPS_L1_CA_CODE_RATE_CHIPS_PER_S;
        let mut code_phase_error: f32 = 0.0;
        let mut skip_samples = 0;
        let mut n = 0;
        'outer: loop {
            let num_ca_code_samples = (f_sampling
                / (f_code / (gps_constants::GPS_L1_CA_CODE_LENGTH_CHIPS - code_phase_error)))
                .ceil() as usize
                + skip_samples;

            let mut length_signal_ms: usize = 1 * 2; // 1 ms complex data
            let mut samples_length = length_signal_ms * num_ca_code_samples;
            if stage == ProcessStage::FinerAcquisitionDoppler {
                length_signal_ms = 11;
                samples_length = length_signal_ms * 2 * num_ca_code_samples;
            }

            let mut buffer: Vec<i8> = Vec::with_capacity(samples_length);
            while buffer.len() < samples_length {
                if let Ok(byte) = f.read_be() {
                    buffer.push(byte);
                    buffer.push(0);
                } else {
                    break 'outer;
                }
            }
            buffer = buffer[2 * skip_samples..].to_vec();
            let buffer_samples: Vec<i16> = buffer.iter().map(|&x| x as i16).collect();

            do_data_process(
                &buffer_samples,
                f_sampling,
                f_inter_freq,
                &mut stage,
                &mut acq_results,
                &mut tracking_statistic,
                length_signal_ms,
                false,
            );
            f_code = *tracking_statistic
                .get(&3)
                .unwrap()
                .tracking_stat
                .code_freq
                .last()
                .unwrap();
            code_phase_error = *tracking_statistic
                .get(&3)
                .unwrap()
                .tracking_stat
                .code_phase_error
                .last()
                .unwrap();
            n += 1;

            // Skip samples for the first time tracking
            if n == 2 {
                for acq_res in &acq_results {
                    if acq_res.prn == 3 {
                        skip_samples = acq_res.code_phase - 1;
                    }
                }
            } else {
                skip_samples = 0;
            }
        }
        plot_samples(&tracking_statistic.get(&3).unwrap().tracking_stat.i_prompt);
    }
}
