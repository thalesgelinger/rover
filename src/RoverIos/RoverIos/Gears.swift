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
    
    @objc public static func createButton() -> UIButton {
        print("CREATE BUTTON ON IOS NATIVE")
        let button = UIButton(type: .system)
        button.frame = CGRect(x: 100, y: 100, width: 150, height: 50)
        button.setTitle("Tap me", for: .normal)
        button.backgroundColor = .systemBlue
        button.setTitleColor(.white, for: .normal)
        button.layer.cornerRadius = 8
        button.addAction(UIAction { _ in
            print("Button Pressed") // Call the callback directly
        }, for: .touchUpInside)
        return button
    }
    
}
