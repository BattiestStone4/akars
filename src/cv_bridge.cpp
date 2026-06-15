#include <cstddef>
#include <cstdint>
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
                                          int* src_h) {
    if (!jpeg || jpeg_len == 0 || !dst || dst_w <= 0 || dst_h <= 0) {
        return -1;
    }

    try {
        std::vector<uint8_t> encoded(jpeg, jpeg + jpeg_len);
        cv::Mat bgr = cv::imdecode(encoded, cv::IMREAD_COLOR);
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

        cv::Mat rgb;
        cv::cvtColor(canvas, rgb, cv::COLOR_BGR2RGB);

        std::vector<cv::Mat> channels;
        cv::split(rgb, channels);
        const size_t channel_size = static_cast<size_t>(dst_w) * static_cast<size_t>(dst_h);
        std::memcpy(dst + 0 * channel_size, channels[0].data, channel_size);
        std::memcpy(dst + 1 * channel_size, channels[1].data, channel_size);
        std::memcpy(dst + 2 * channel_size, channels[2].data, channel_size);
        return 0;
    } catch (const std::exception&) {
        return -3;
    } catch (...) {
        return -4;
    }
}
