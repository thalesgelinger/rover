//
//  ViewController.swift
//  ios
//
//  Created by Thales Gelinger on 20/06/24.
//

import UIKit

class ViewController: UIViewController {
    
    private let gears = Gears()

    override func viewDidLoad() {
        super.viewDidLoad()

        let containerView = UIView(frame: view.bounds)
        containerView.backgroundColor = .white

        let label = UILabel(frame: .zero)
        
        label.text = gears.greetings(to: "world")
        
        label.textAlignment = .center

        label.sizeToFit()

        label.center = containerView.center

        containerView.addSubview(label)

        view.addSubview(containerView)
    }

}
