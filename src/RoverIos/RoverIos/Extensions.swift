//
//  Extensions.swift
//  RoverIos
//
//  Created by Thales Gelinger on 16/07/24.
//

import Foundation
import UIKit

extension UIColor {
    convenience init?(hex: String) {
        let r, g, b, a: CGFloat

        var hexColor = hex.trimmingCharacters(in: .whitespacesAndNewlines)
        if hexColor.hasPrefix("#") {
            hexColor.remove(at: hexColor.startIndex)
        }

        // Handle 6 or 8 character hex
        guard hexColor.count == 6 || hexColor.count == 8 else {
            return nil
        }

        let scanner = Scanner(string: hexColor)
        var hexNumber: UInt64 = 0

        if scanner.scanHexInt64(&hexNumber) {
            if hexColor.count == 8 {
                r = CGFloat((hexNumber & 0xff000000) >> 24) / 255
                g = CGFloat((hexNumber & 0x00ff0000) >> 16) / 255
                b = CGFloat((hexNumber & 0x0000ff00) >> 8) / 255
                a = CGFloat(hexNumber & 0x000000ff) / 255
            } else { // 6 characters
                r = CGFloat((hexNumber & 0x00ff0000) >> 16) / 255
                g = CGFloat((hexNumber & 0x0000ff00) >> 8) / 255
                b = CGFloat(hexNumber & 0x000000ff) / 255
                a = 1.0 // Default alpha for RGB
            }

            self.init(red: r, green: g, blue: b, alpha: a)
            return
        }

        return nil
    }
}
