//
//  ViewController.swift
//  veilidtools-tests
//
//  Created by JSmith on 6/27/21.
//

import UIKit
import Darwin

class ViewController: UIViewController {

    override func viewDidLoad() {
        super.viewDidLoad()
        // Off the main thread: on a device the watchdog kills an app that blocks it through launch
        DispatchQueue.global(qos: .userInitiated).async {
            run_veilid_tools_tests()
            exit(0)
        }
    }


}

