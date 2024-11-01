//
//  Gears.swift
//  RoverIos
//
//  Created by Thales Gelinger on 12/07/24.
//

import Foundation

import UIKit

@objc public class Gears: NSObject {
    @objc public static func createView(_ props: String) -> UIView {
        print(props)
        let containerView = UIView()
        let viewProps = ViewProps.fromJSON(props)
        if viewProps == nil {
            return containerView
        }
        
        let width = Utils.getWidthValue(viewProps!.width)
        let height = Utils.getHeightValue(viewProps!.height)

        containerView.frame = CGRect(origin: .zero, size: CGSize(
            width: width,
            height:height))
        
        let color = UIColor(hex: viewProps!.color ?? "#FFFFFF")
        containerView.backgroundColor = color
        return containerView
    }
    
    @objc public static func createTextView(_ text: String) -> UILabel {
        let label = UILabel()
        label.text = text
        label.textAlignment = .center
        label.sizeToFit()
        return label
    }
    
    @objc func createButton() -> UIButton {
        let button = UIButton(type: .system)
        button.setTitle("Tap me", for: .normal)
        
        // Configure button appearance
        button.backgroundColor = .systemBlue
        button.setTitleColor(.white, for: .normal)
        button.layer.cornerRadius = 8
        
        // Set up button action to call `buttonTapped` method when pressed
        button.addTarget(self, action: #selector(buttonTapped), for: .touchUpInside)
        
        return button
    }
    
    @objc private func buttonTapped() {
        // Print a message to the console when the button is pressed
        print("Button was tapped!")
    }
}
