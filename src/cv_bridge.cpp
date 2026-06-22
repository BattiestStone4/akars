#include <chrono>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <exception>
#include <vector>

#include <opencv2/core.hpp>
#include <opencv2/imgcodecs.hpp>
#include <opencv2/imgproc.hpp>

extern "C" int akars_mjpeg_to_rgb_planar(const uint8_t* jpeg,
                                          size_t jpeg_len,
                                          uint8_t* dst,
                                          int dst_w,
                                          int dst_h,
                                          int* src_w,
                                          int* src_h,
                                          int64_t* decode_us,
                                          int64_t* resize_us,
                                          int64_t* pack_us) {
    if (!jpeg || jpeg_len == 0 || !dst || dst_w <= 0 || dst_h <= 0) {
        return -1;
    }

    using clock = std::chrono::steady_clock;
    auto since = [](clock::time_point a, clock::time_point b) {
        return std::chrono::duration_cast<std::chrono::microseconds>(b - a).count();
    };

    try {
        std::vector<uint8_t> encoded(jpeg, jpeg + jpeg_len);
        const auto t0 = clock::now();
        cv::Mat bgr = cv::imdecode(encoded, cv::IMREAD_COLOR);
        const auto t1 = clock::now();
        if (bgr.empty()) {
            return -2;
        }

        if (src_w) *src_w = bgr.cols;
        if (src_h) *src_h = bgr.rows;

        const double scale = std::min(static_cast<double>(dst_w) / bgr.cols,
                                      static_cast<double>(dst_h) / bgr.rows);
        const int resized_w = std::max(1, static_cast<int>(bgr.cols * scale));
        const int resized_h = std::max(1, static_cast<int>(bgr.rows * scale));
        const int pad_left = (dst_w - resized_w) / 2;
        const int pad_top = (dst_h - resized_h) / 2;

        cv::Mat resized;
        cv::resize(bgr, resized, cv::Size(resized_w, resized_h), 0, 0, cv::INTER_LINEAR);

        cv::Mat canvas(dst_h, dst_w, CV_8UC3, cv::Scalar(0, 0, 0));
        resized.copyTo(canvas(cv::Rect(pad_left, pad_top, resized_w, resized_h)));
        const auto t2 = clock::now();

        cv::Mat rgb;
        cv::cvtColor(canvas, rgb, cv::COLOR_BGR2RGB);

        std::vector<cv::Mat> channels;
        cv::split(rgb, channels);
        const size_t channel_size = static_cast<size_t>(dst_w) * static_cast<size_t>(dst_h);
        std::memcpy(dst + 0 * channel_size, channels[0].data, channel_size);
        std::memcpy(dst + 1 * channel_size, channels[1].data, channel_size);
        std::memcpy(dst + 2 * channel_size, channels[2].data, channel_size);
        const auto t3 = clock::now();

        if (decode_us) *decode_us = since(t0, t1);
        if (resize_us) *resize_us = since(t1, t2);
        if (pack_us) *pack_us = since(t2, t3);
        return 0;
    } catch (const std::exception&) {
        return -3;
    } catch (...) {
        return -4;
    }
}

// Decode an image, draw the given detection boxes on top of it, and write the
// annotated result to out_path. Boxes are [n][4] center-form (cx, cy, w, h) in
// original-image pixel coordinates, matching the output of correct_yolo_boxes.
extern "C" int akars_draw_detections(const uint8_t* image,
                                     size_t image_len,
                                     const float* boxes,
                                     const int* classes,
                                     const float* scores,
                                     int count,
                                     const char* out_path) {
    if (!image || image_len == 0 || !out_path || count < 0) {
        return -1;
    }
    if (count > 0 && (!boxes || !classes || !scores)) {
        return -1;
    }

    try {
        std::vector<uint8_t> encoded(image, image + image_len);
        cv::Mat bgr = cv::imdecode(encoded, cv::IMREAD_COLOR);
        if (bgr.empty()) {
            return -2;
        }

        for (int i = 0; i < count; ++i) {
            const float cx = boxes[i * 4 + 0];
            const float cy = boxes[i * 4 + 1];
            const float w = boxes[i * 4 + 2];
            const float h = boxes[i * 4 + 3];
            const int x1 = static_cast<int>(cx - w / 2.0f);
            const int y1 = static_cast<int>(cy - h / 2.0f);
            const int x2 = static_cast<int>(cx + w / 2.0f);
            const int y2 = static_cast<int>(cy + h / 2.0f);

            const cv::Scalar color(0, 255, 0);
            cv::rectangle(bgr, cv::Point(x1, y1), cv::Point(x2, y2), color, 2);

            char label[64];
            std::snprintf(label, sizeof(label), "%d %.2f", classes[i], scores[i]);
            int baseline = 0;
            const cv::Size text_size =
                cv::getTextSize(label, cv::FONT_HERSHEY_SIMPLEX, 0.5, 1, &baseline);
            const int label_top = std::max(0, y1 - text_size.height - baseline);
            cv::rectangle(bgr,
                          cv::Point(x1, label_top),
                          cv::Point(x1 + text_size.width, label_top + text_size.height + baseline),
                          color,
                          cv::FILLED);
            cv::putText(bgr,
                        label,
                        cv::Point(x1, label_top + text_size.height),
                        cv::FONT_HERSHEY_SIMPLEX,
                        0.5,
                        cv::Scalar(0, 0, 0),
                        1);
        }

        if (!cv::imwrite(out_path, bgr)) {
            return -5;
        }
        return 0;
    } catch (const std::exception&) {
        return -3;
    } catch (...) {
        return -4;
    }
}
