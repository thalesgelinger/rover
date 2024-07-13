//
//  Gears.swift
//  RoverIos
//
//  Created by Thales Gelinger on 12/07/24.
//

import Foundation

import UIKit

@objc public class Gears: NSObject {
    @objc public static func createView(_ view: UIView) {
        
        let containerView = UIView(frame: view.bounds)
        containerView.backgroundColor = .white
        view.addSubview(containerView)

        createTextView(view)
    }

    @objc public static func createTextView(_ parent: UIView) {
        let label = UILabel(frame: .zero)
        
        label.text = "Rover Test"
        
        label.textAlignment = .center

        label.sizeToFit()

        label.center = parent.center
        parent.addSubview(label)
    }
}
