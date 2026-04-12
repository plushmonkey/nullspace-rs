use wasm_bindgen::prelude::*;

use wasm_bindgen_futures::JsFuture;

use thiserror::Error;

// Future for setting img.src and kicking off a browser load.
// This will wake up upon successful load or error.
pub struct ImageFuture {
    image: Option<web_sys::HtmlImageElement>,
    load_failed: std::rc::Rc<std::cell::Cell<bool>>,
}

impl ImageFuture {
    pub fn new(url: &str) -> Self {
        let image = web_sys::HtmlImageElement::new().unwrap();

        image.set_src(url);

        Self {
            image: Some(image),
            load_failed: std::rc::Rc::new(std::cell::Cell::new(false)),
        }
    }
}

impl Future for ImageFuture {
    type Output = Result<web_sys::HtmlImageElement, ()>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match &self.image {
            Some(image) if image.complete() => {
                let image = self.image.take().unwrap();
                let failed = self.load_failed.get();

                if failed {
                    std::task::Poll::Ready(Err(()))
                } else {
                    std::task::Poll::Ready(Ok(image))
                }
            }
            Some(image) => {
                use wasm_bindgen::JsCast;

                // Wake up when image.onload is fired in js
                {
                    let waker = cx.waker().clone();

                    let on_load_closure = Closure::wrap(Box::new(move || {
                        waker.wake_by_ref();
                    }) as Box<dyn FnMut()>);

                    image.set_onload(Some(on_load_closure.as_ref().unchecked_ref()));
                    on_load_closure.forget();
                }

                // Wake up if image.onerror is fired in js
                {
                    let waker = cx.waker().clone();
                    let failed = self.load_failed.clone();

                    let on_error_closure = Closure::wrap(Box::new(move || {
                        failed.set(true);
                        waker.wake_by_ref();
                    })
                        as Box<dyn FnMut()>);

                    image.set_onerror(Some(on_error_closure.as_ref().unchecked_ref()));
                    on_error_closure.forget();
                }

                std::task::Poll::Pending
            }
            _ => std::task::Poll::Ready(Err(())),
        }
    }
}

#[derive(Error, Debug)]
pub enum ImageLoadError {
    #[error("img load failed")]
    ImageLoadError,

    #[error("img bitmap create failed")]
    CreateImageBitmapError,
    #[error("img bitmap load failed")]
    LoadImageBitmapError,

    #[error("offscreen canvas create failed")]
    CanvasCreateError,

    #[error("offscreen canvas context create failed")]
    CanvasContextCreateError,

    #[error("canvas.get_image_data failed")]
    CanvasGetImageData,
}

pub async fn load_image(url: &str) -> Result<web_sys::ImageData, ImageLoadError> {
    let img_element = match ImageFuture::new(url).await {
        Ok(img) => img,
        Err(_) => return Err(ImageLoadError::ImageLoadError),
    };

    let promise = match web_sys::window()
        .unwrap()
        .create_image_bitmap_with_html_image_element(&img_element)
    {
        Ok(promise) => promise,
        Err(_) => return Err(ImageLoadError::CreateImageBitmapError),
    };

    let value = match JsFuture::from(promise).await {
        Ok(value) => value,
        Err(_) => return Err(ImageLoadError::LoadImageBitmapError),
    };

    let image_bitmap: web_sys::ImageBitmap = value.dyn_into().unwrap();

    let offscreen_canvas =
        web_sys::OffscreenCanvas::new(image_bitmap.width(), image_bitmap.height()).unwrap();

    let context: web_sys::OffscreenCanvasRenderingContext2d =
        match offscreen_canvas.get_context("2d") {
            Ok(context) => context,
            Err(_) => return Err(ImageLoadError::CanvasContextCreateError),
        }
        .unwrap()
        .dyn_into()
        .unwrap();

    let _ = context.draw_image_with_image_bitmap(&image_bitmap, 0.0, 0.0);
    let image_data = match context.get_image_data(
        0,
        0,
        image_bitmap.width() as i32,
        image_bitmap.height() as i32,
    ) {
        Ok(image_data) => image_data,
        Err(_) => return Err(ImageLoadError::CanvasGetImageData),
    };

    Ok(image_data)
}

#[wasm_bindgen]
extern "C" {
    fn setInterval(closure: &Closure<dyn FnMut()>, millis: u32) -> f64;
    fn clearInterval(token: f64);
}

#[wasm_bindgen]
pub struct Interval {
    _closure: Closure<dyn FnMut()>,
    token: f64,
}

impl Interval {
    pub fn new<F: 'static>(millis: u32, f: F) -> Interval
    where
        F: FnMut(),
    {
        let closure = Closure::new(f);
        let token = setInterval(&closure, millis);

        Interval {
            _closure: closure,
            token,
        }
    }
}

impl Drop for Interval {
    fn drop(&mut self) {
        clearInterval(self.token);
    }
}
