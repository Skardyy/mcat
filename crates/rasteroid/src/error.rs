use thiserror::Error;

#[derive(Debug, Error)]
pub enum RasterError {
    #[error(
        "Provided dimension is not a valid format. valid formats are: N%, Nc, Npx, N. 0 is not valid."
    )]
    InvalidDimensionFormat,
    #[error(
        "Provided size is not a valid format. valid format is: widthxheight e.g. 1920x1080, or 1920xauto, autox1080, autoxauto"
    )]
    InvalidSizeFormat,

    #[error("image error: {0}")]
    ImageError(#[from] image::ImageError),

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Image width or height is 0")]
    EmptyImage,

    #[error("There were no frames to encode")]
    EmptyVideo,

    #[error("Couldn't identify the image color type")]
    InvalidImage,

    #[error("shared memory error: {0}")]
    ShmemError(#[from] shared_memory::ShmemError),

    #[error("resize error: {0}")]
    ResizeError(#[from] fast_image_resize::ResizeError),
}
