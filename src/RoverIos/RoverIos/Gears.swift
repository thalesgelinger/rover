//
//  Gears.swift
//  RoverIos
//
//  Created by Thales Gelinger on 12/07/24.
//

import Foundation

import UIKit

@objc public class Gears: NSObject {
    @objc public static func createView() -> UIView {
        let containerView = UIView(frame: CGRect(origin: .zero, size: CGSize(width: 100, height: 200)))
        containerView.backgroundColor = .white
        return containerView
    }

    @objc public static func createTextView(_ text: String) -> UILabel {
        let label = UILabel()
        label.text = text
        label.textAlignment = .center
        label.frame = CGRect(x: 0, y: 0, width: 100, height: 50)
        return label
    }
}
