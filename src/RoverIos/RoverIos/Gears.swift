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
}
