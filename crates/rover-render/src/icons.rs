use skia_safe::{Path, Point};

pub struct IconPaths;

impl IconPaths {
    pub fn get(name: &str, size: f32) -> Option<Path> {
        let scale = size / 24.0;
        let path_data = match name {
            "user" => "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2M12 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z",
            "check" => "M20 6 9 17l-5-5",
            "x" => "m18 6-12 12M6 6l12 12",
            "search" => "m21 21-6-6m2-5a7 7 0 1 1-14 0 7 7 0 0 1 14 0Z",
            "home" => "m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
            "settings" => "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
            "heart" => "M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z",
            "star" => "M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z",
            "menu" => "M4 6h16M4 12h16M4 18h16",
            "plus" => "M5 12h14m-7-7v14",
            "minus" => "M5 12h14",
            "chevron-down" => "m6 9 6 6 6-6",
            "chevron-up" => "m18 15-6-6-6 6",
            "chevron-left" => "m15 18-6-6 6-6",
            "chevron-right" => "m9 18 6-6-6-6",
            "mail" => "M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z M22 6l-10 7L2 6",
            "bell" => "M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9m-5 13a2 2 0 0 1-2 2 2 2 0 0 1-2-2",
            "calendar" => "M8 2v4m8-4v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z",
            "clock" => "M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10zM12 6v6l4 2",
            "info" => "M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10zm0-10v5m0-9.01V8",
            "alert-circle" => "M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10zm0-7v-5m0 9.01V19",
            "loader" => "M12 2v4m0 12v4M4.93 4.93l2.83 2.83m8.48 8.48 2.83 2.83M2 12h4m12 0h4M4.93 19.07l2.83-2.83m8.48-8.48 2.83-2.83",
            "file" => "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6",
            "download" => "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4m4-5 5 5 5-5m-5 5V3",
            "upload" => "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4m14-7-5-5-5 5m5-5v12",
            "trash" => "M3 6h18m-2 0v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6m3 0V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2",
            "edit" => "M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z",
            "eye" => "M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7zm10 3a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
            "eye-off" => "M9.88 9.88a3 3 0 1 0 4.24 4.24M10.73 5.08A10.43 10.43 0 0 1 12 5c7 0 10 7 10 7a13.16 13.16 0 0 1-1.67 2.68M6.61 6.61A13.526 13.526 0 0 0 2 12s3 7 10 7a9.74 9.74 0 0 0 5.39-1.61M2 2l20 20",
            _ => return None,
        };

        Some(Self::parse_svg_path(path_data, scale))
    }

    fn parse_svg_path(data: &str, scale: f32) -> Path {
        let mut path = Path::new();
        let tokens: Vec<&str> = data.split_whitespace().collect();
        let mut i = 0;
        let mut current = Point::new(0.0, 0.0);

        while i < tokens.len() {
            let cmd = tokens[i];
            i += 1;

            match cmd {
                "M" | "m" => {
                    if i + 1 < tokens.len() {
                        if let (Ok(x), Ok(y)) = (tokens[i].parse::<f32>(), tokens[i + 1].parse::<f32>()) {
                            let pt = if cmd == "M" {
                                Point::new(x * scale, y * scale)
                            } else {
                                Point::new(current.x + x * scale, current.y + y * scale)
                            };
                            path.move_to(pt);
                            current = pt;
                            i += 2;
                        }
                    }
                }
                "L" | "l" => {
                    if i + 1 < tokens.len() {
                        if let (Ok(x), Ok(y)) = (tokens[i].parse::<f32>(), tokens[i + 1].parse::<f32>()) {
                            let pt = if cmd == "L" {
                                Point::new(x * scale, y * scale)
                            } else {
                                Point::new(current.x + x * scale, current.y + y * scale)
                            };
                            path.line_to(pt);
                            current = pt;
                            i += 2;
                        }
                    }
                }
                "H" | "h" => {
                    if i < tokens.len() {
                        if let Ok(x) = tokens[i].parse::<f32>() {
                            let pt = if cmd == "H" {
                                Point::new(x * scale, current.y)
                            } else {
                                Point::new(current.x + x * scale, current.y)
                            };
                            path.line_to(pt);
                            current = pt;
                            i += 1;
                        }
                    }
                }
                "V" | "v" => {
                    if i < tokens.len() {
                        if let Ok(y) = tokens[i].parse::<f32>() {
                            let pt = if cmd == "V" {
                                Point::new(current.x, y * scale)
                            } else {
                                Point::new(current.x, current.y + y * scale)
                            };
                            path.line_to(pt);
                            current = pt;
                            i += 1;
                        }
                    }
                }
                "Z" | "z" => {
                    path.close();
                }
                "a" => {
                    if i + 6 < tokens.len() {
                        let rx = tokens[i].parse::<f32>().unwrap_or(0.0) * scale;
                        let ry = tokens[i + 1].parse::<f32>().unwrap_or(0.0) * scale;
                        let _rotation = tokens[i + 2].parse::<f32>().unwrap_or(0.0);
                        let _large_arc = tokens[i + 3].parse::<i32>().unwrap_or(0);
                        let _sweep = tokens[i + 4].parse::<i32>().unwrap_or(0);
                        let dx = tokens[i + 5].parse::<f32>().unwrap_or(0.0) * scale;
                        let dy = tokens[i + 6].parse::<f32>().unwrap_or(0.0) * scale;
                        
                        let end = Point::new(current.x + dx, current.y + dy);
                        let ctrl1 = Point::new(current.x + rx * 0.5, current.y);
                        let ctrl2 = Point::new(end.x, end.y - ry * 0.5);
                        path.cubic_to(ctrl1, ctrl2, end);
                        current = end;
                        i += 7;
                    }
                }
                "A" => {
                    if i + 6 < tokens.len() {
                        let rx = tokens[i].parse::<f32>().unwrap_or(0.0) * scale;
                        let ry = tokens[i + 1].parse::<f32>().unwrap_or(0.0) * scale;
                        let _rotation = tokens[i + 2].parse::<f32>().unwrap_or(0.0);
                        let _large_arc = tokens[i + 3].parse::<i32>().unwrap_or(0);
                        let _sweep = tokens[i + 4].parse::<i32>().unwrap_or(0);
                        let x = tokens[i + 5].parse::<f32>().unwrap_or(0.0) * scale;
                        let y = tokens[i + 6].parse::<f32>().unwrap_or(0.0) * scale;
                        
                        let end = Point::new(x, y);
                        let ctrl1 = Point::new(current.x + rx * 0.5, current.y);
                        let ctrl2 = Point::new(end.x, end.y - ry * 0.5);
                        path.cubic_to(ctrl1, ctrl2, end);
                        current = end;
                        i += 7;
                    }
                }
                "c" => {
                    if i + 5 < tokens.len() {
                        if let (Ok(x1), Ok(y1), Ok(x2), Ok(y2), Ok(x), Ok(y)) = (
                            tokens[i].parse::<f32>(),
                            tokens[i + 1].parse::<f32>(),
                            tokens[i + 2].parse::<f32>(),
                            tokens[i + 3].parse::<f32>(),
                            tokens[i + 4].parse::<f32>(),
                            tokens[i + 5].parse::<f32>(),
                        ) {
                            let ctrl1 = Point::new(current.x + x1 * scale, current.y + y1 * scale);
                            let ctrl2 = Point::new(current.x + x2 * scale, current.y + y2 * scale);
                            let end = Point::new(current.x + x * scale, current.y + y * scale);
                            path.cubic_to(ctrl1, ctrl2, end);
                            current = end;
                            i += 6;
                        }
                    }
                }
                _ => {}
            }
        }

        path
    }
}
