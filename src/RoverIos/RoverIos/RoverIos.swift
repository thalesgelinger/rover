//
//  RoverIos.swift
//  RoverIos
//
//  Created by Thales Gelinger on 04/07/24.
//

import Foundation
import RoverIos.Gears
import UIKit

func swiftHello(_ str: String) -> String {
    let result = gretting(str)
    let swift_result = String(cString: result!)
    print("Swift Received: \(swift_result)")
    return swift_result
}


open class RoverIos: UIViewController {
    public override func viewDidLoad() {
        super.viewDidLoad()

        start()
    }
    
    func start() {
        let containerView = UIView(frame: view.bounds)
        containerView.backgroundColor = .white

        let label = UILabel(frame: .zero)
        
        label.text = swiftHello("Rover")
        
        label.textAlignment = .center

        label.sizeToFit()

        label.center = containerView.center

        containerView.addSubview(label)

        view.addSubview(containerView)
    }
}
